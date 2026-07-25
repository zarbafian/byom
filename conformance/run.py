#!/usr/bin/env python3
"""B0.1 conformance runner for the BPP spec tree.

    python3 conformance/run.py            # spec/ next to this file's parent
    python3 conformance/run.py path/to/spec

Checks, in order:

1. every file in spec/schemas/ (recursively) parses as strict I-JSON, follows
   the spec conventions (draft 2020-12, $id present, closed objects, no
   remote $ref, resolvable internal $refs, compilable patterns), and
   compiles — with `jsonschema` when installed, otherwise against this
   file's minimal structural validator;
2. the complete B0.1 bundle op list — transcribed from the
   plan/sheets/B0.1.md family lists (the interim freeze source until
   spec/registry/ lands) and cross-checked against the §14.6 catalog — is
   schema-covered: every op has a closed <op>-request and <op>-result
   schema, the request pins the exact op const, mutations require meta and
   reads carry none;
3. every descriptor in spec/descriptors/ is structurally valid
   ({machine, states, transitions}), every via references a real catalog
   operation or a named kernel/server transition, and descriptor parity
   holds: every mutating operation in the slice appears in exactly one
   descriptor's owning (non-cascade) transitions (§14.8 one-to-one rule);
   cascade transitions must cite an operation owned by another descriptor;
4. every vector in spec/vectors/ passes: schema vectors match their expected
   verdict (bpp-failure additionally cross-checks exact type/kind agreement,
   which JSON Schema cannot express); acceptance vectors match the C1 family
   acceptance rules of family-vectors/PROFILE.md section 1 (token-order first
   error, 256 KiB request / 1 MiB response caps, inclusive depth-64 and
   65 536-node caps, `$domain` reservation at every depth, surrogate and
   float rules — R0/BYOM-03); digest vectors re-derive $domain-tagged JCS
   canonical bytes and the keyed scope_erasure_safe HMAC DigestRef
   (PROFILE.md sections 2/5/6, normative; D-R0-1 — R0/BYOM-01);
5. machine state-walk vectors (spec/vectors/machines/) replay crash/replay
   transition sequences through a small interpreter over the committed
   descriptor JSON: accepted steps must be exact descriptor rows, rejected
   steps must be absent rows (the §14.8 closed-machine rule: an unlisted
   transition is invalid), replay steps must be state-idempotent, and a
   {"crash": true} marker restarts the daemon without moving the durable
   descriptor-level state;
6. BPA-1 policy vectors (spec/vectors/policy/, `input.policy_op` — ADR-0001
   accepted, DESIGN.md §10.5): every well_formed/canonical/intersect/
   is_subset/decide case re-derives through the reference evaluator
   policy/eval.py and must equal the golden `expected.result` exactly
   (typed rejections included). When `node` is on PATH the same cases are
   additionally replayed through the independent evaluator policy/eval.mjs
   in one batch and every result must agree byte-for-byte (JCS) with the
   Python result — the B0.1 "two independent policy evaluators" gate;
   without node, run-checks.sh's dedicated eval.mjs and differential steps
   still enforce it;
7. the C3a MCP tool bundle (mcp/byom-mcp.tools.json) validates against the
   closed meta-schema spec/schemas/mcp-tools.schema.json; each profile's op
   list EXACTLY equals the C3a sheet list transcribed below
   (plan/sheets/C3a.md; candidate binding normative via byom amendment A4);
   every tool's input-schema fields are a subset of its committed op request
   schema's args (envelope {version, op, meta} excluded — the byom-mcp
   bridge derives them from the channel) with no invented and no
   channel-derived (G16) fields; reads are safe_to_allow and mutations
   gated (the akson-mcp marking); and zero governance, runtime, or admin
   operations are bound. Tool-call vectors (spec/vectors/mcp/) replay
   call shapes against the committed document.
8. the C2 governed-work family (spec/governed-work/, byom §16.3/§16.6 plus
   the slice-2 sources §7.4/§11.4/§12.1/§14.3/§17.2; family contract
   §2.A-2.F/2.J, Δ4/Δ5): the closed record-schema inventory (slice 1 + the
   slice-2 episode/effect/driver contracts) is present and compiled; every
   design state/status enum equals its transcription below verbatim (order
   included; array fields under items); the Kovee-owned executor
   descriptors exist, declare owner "kovee (C2)", and the pinned state
   lists (formation §16.3, subordinate bridge §11.4, dispatch head §17.2)
   are verbatim. Kovee-owned descriptors never own a BPP operation, so the
   exactly-once descriptor-parity rule below is not polluted. Cross-member
   checks JSON Schema cannot express (the bpp-failure type/kind pattern):
   restore-lineage-proof hop_count equals the ordered_hops length, and
   byom-subordinate-reservation items never exceed or reshape their parent
   dimension (§11.4 never-above-parent). The Δ4 act-class-subject taxonomy
   is cross-validated against BPA-1 twice: statically (its oneOf arms
   encode exactly the transcribed mandatory-domain table and its copied
   $defs are byte-identical to bpa1-policy.schema.json) and dynamically
   (every act-class-subject vector's atoms replay through the policy/eval.py
   reference evaluator; a schema-valid subject the evaluator rejects is a
   hard divergence failure).

Exit code 0 only when everything passes. Self-contained: Python stdlib only,
with `jsonschema` used opportunistically when installed and `node` used
opportunistically for the cross-evaluator policy check.
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import re
import sys
from pathlib import Path

SAFE_MAX = 2**53 - 1
REQUEST_CAP = 262144      # §14.9 / PROFILE.md §1: request envelope at most 256 KiB
RESPONSE_CAP = 1048576    # §14.9 / PROFILE.md §1: response at most 1 MiB
DEPTH_CAP = 64            # PROFILE.md profile-pinned decision 1 (inclusive)
NODE_CAP = 65536          # PROFILE.md profile-pinned decision 1 (inclusive)
DRAFT = "https://json-schema.org/draft/2020-12/schema"

# ------------------------------------------------------ operation catalog ----
# The §14.6 operation catalog, transcribed per family. This is the interim
# machine-readable op list until spec/registry/ lands (later B0.1 slice);
# bundle membership and counts derive from it, never from prose.
CATALOG = {
    "negotiation": ("hello", "protocol_info", "feature_info"),
    "society": ("society_prepare", "society_bootstrap", "society_show",
                "society_hold", "society_release", "society_dissolve"),
    "charter": ("charter_propose", "charter_position", "charter_finalize",
                "charter_history"),
    "participants": ("participant_propose", "membership_offer",
                     "membership_offer_revoke", "onboarding_offer",
                     "participant_admit", "participant_show",
                     "participant_suspend", "participation_cease",
                     "participant_retire", "manifestation_propose",
                     "manifestation_admit", "manifestation_disable",
                     "assent_policy_adopt", "assent_policy_revoke",
                     "activation_policy_adopt", "activation_policy_revoke",
                     "continuity_root_update"),
    "candidates": ("membership_refuse", "membership_accept",
                   "candidate_self_policy_propose"),
    "control": ("control_domain_propose", "control_domain_position",
                "control_domain_finalize", "control_domain_merge"),
    "procedures": ("procedure_propose", "procedure_position",
                   "procedure_finalize", "procedure_hold",
                   "procedure_release"),
    "assemblies": ("formation_start", "formation_revise", "assembly_propose",
                   "assembly_position", "assembly_finalize", "assembly_hold",
                   "assembly_reform", "assembly_withdraw", "assembly_dissolve",
                   "collective_policy_propose", "collective_decision_finalize"),
    "endeavors": ("endeavor_propose", "endeavor_position", "endeavor_finalize",
                  "endeavor_hold", "endeavor_release", "endeavor_close"),
    "calls_and_pledges": ("call_open", "call_withdraw", "pledge_propose",
                          "pledge_position", "pledge_finalize", "pledge_amend",
                          "pledge_resume", "pledge_relinquish",
                          "delivery_submit", "delivery_withdraw",
                          "review_record"),
    "mandates": ("mandate_prepare", "mandate_position", "mandate_issue",
                 "mandate_derive", "mandate_hold", "mandate_revoke",
                 "standing_mandate_prepare", "standing_mandate_position",
                 "standing_mandate_issue", "standing_mandate_hold",
                 "standing_mandate_revoke"),
    "acts": ("act_intent_prepare", "act_intent_position",
             "act_intent_finalize", "act_intent_cancel",
             "execution_permit_consume"),
    "disputes": ("dispute_raise", "dispute_position", "dispute_hold",
                 "dispute_resolve", "appeal_raise", "appeal_position",
                 "appeal_resolve"),
    "activities": ("activity_open", "activity_show", "activity_hold",
                   "activity_close", "wake_intent_submit",
                   "wake_intent_withdraw", "episode_request",
                   "continuation_write"),
    "runtime": ("onboarding_episode_claim", "onboarding_compute_permit_consume",
                "onboarding_episode_complete", "placement_admit",
                "episode_claim", "episode_start", "checkpoint_commit",
                "episode_yield", "episode_complete", "episode_fail",
                "usage_report", "effect_outcome_admit"),
    "knowledge": ("engram_propose", "engram_admit", "engram_read",
                  "engram_search", "engram_attest", "engram_hold",
                  "engram_retire", "context_manifest_show"),
    "classification": ("classification_overlay_propose",
                       "classification_mapping_propose",
                       "outbound_classification_propose",
                       "classification_position", "classification_finalize",
                       "classification_revoke"),
    "privacy_lifecycle": ("erasure_request", "erasure_position",
                          "erasure_finalize", "erasure_execute",
                          "erasure_verify"),
    "budgets": ("budget_show", "budget_reservation_show",
                "usage_settlement_show", "budget_reconcile"),
    "events": ("snapshot_get", "events_read", "events_wait", "event_payload"),
    "host_integration": ("kovee_endeavor_form",),
    "recovery": ("idempotency_result", "external_command_result_query",
                 "external_command_terminalize", "effect_reconcile",
                 "cursor_recover", "recovery_checkpoint_show"),
    "administration": ("operational_hold", "operational_release", "diagnose",
                       "backup", "restore", "key_configure",
                       "service_configure"),
}
ALL_CATALOG_OPS = frozenset(op for ops in CATALOG.values() for op in ops)

# ------------------------------------------------------ B0.1 sheet bundle ----
# The complete B0.1 bundle, transcribed verbatim from the family lists in
# plan/sheets/B0.1.md (the interim freeze source until spec/registry/ lands;
# counts are sheet-derived, never prose). Standing mandates stay out per byom
# amendment A7 (B0.2); the sheet's recovery core deliberately excludes
# external_command_*/effect_reconcile (later bundles). After slice 3
# (mandates, acts, charter, events + recovery core) every op below must have
# a schema pair — check_bundle fails on any gap.
B01_SHEET = {
    "negotiation": ("hello", "protocol_info", "feature_info"),
    "society": ("society_prepare", "society_bootstrap", "society_show",
                "society_hold", "society_release", "society_dissolve"),
    "charter": ("charter_propose", "charter_position", "charter_finalize",
                "charter_history"),
    "participants": ("participant_propose", "membership_offer",
                     "membership_offer_revoke", "onboarding_offer",
                     "participant_admit", "participant_show",
                     "participant_suspend", "participation_cease",
                     "participant_retire", "manifestation_propose",
                     "manifestation_admit", "manifestation_disable",
                     "assent_policy_adopt", "assent_policy_revoke",
                     "activation_policy_adopt", "activation_policy_revoke",
                     "continuity_root_update"),
    "candidates": ("membership_refuse", "membership_accept",
                   "candidate_self_policy_propose"),
    "endeavors": ("endeavor_propose", "endeavor_position",
                  "endeavor_finalize", "endeavor_hold", "endeavor_release",
                  "endeavor_close"),
    "calls_and_pledges": ("call_open", "call_withdraw", "pledge_propose",
                          "pledge_position", "pledge_finalize",
                          "pledge_amend", "pledge_resume",
                          "pledge_relinquish", "delivery_submit",
                          "delivery_withdraw", "review_record"),
    "mandates": ("mandate_prepare", "mandate_position", "mandate_issue",
                 "mandate_derive", "mandate_hold", "mandate_revoke"),
    "acts": ("act_intent_prepare", "act_intent_position",
             "act_intent_finalize", "act_intent_cancel",
             "execution_permit_consume"),
    "activities": ("activity_open", "activity_show", "activity_hold",
                   "activity_close", "wake_intent_submit",
                   "wake_intent_withdraw", "episode_request",
                   "continuation_write"),
    "events_and_recovery_core": ("snapshot_get", "events_read", "events_wait",
                                 "event_payload", "idempotency_result",
                                 "cursor_recover",
                                 "recovery_checkpoint_show"),
}
SLICE_OPS = tuple(op for ops in B01_SHEET.values() for op in ops)
# Reads never mutate and never carry meta (§14.2). idempotency_result and
# cursor_recover are classed as reads: R41 "never re-executes" (gap note G40
# in spec/schemas/ops/README.md); charter_history and the events family are
# R4 projection reads; negotiation is pre-auth read-only (R1).
SLICE_READS = frozenset({
    "hello", "protocol_info", "feature_info",
    "society_show", "participant_show", "activity_show",
    "charter_history", "snapshot_get", "events_read", "events_wait",
    "event_payload", "idempotency_result", "cursor_recover",
    "recovery_checkpoint_show",
})
SLICE_MUTATING = tuple(op for op in SLICE_OPS if op not in SLICE_READS)

# Named non-callable kernel/server transitions that may appear as a
# descriptor `via` (§14.8, spec/README.md). `standing_replacement` is the
# gap-note G12 name for the Standing row's operation-less 'replacement';
# `pledge_disposition_decision` is the gap-note G22 name for the Pledge
# row's operation-less 'decision' (→ canceled/failed);
# `host_effect_attempt` is the gap-note G36 name for the ActIntent row's
# operation-less 'host attempt' (consumed → executing, Kovee-owned).
NAMED_TRANSITIONS = frozenset({
    "server_time", "activation_admit", "resource_allocate",
    "standing_replacement", "pledge_disposition_decision",
    "host_effect_attempt",
    # §15.3 internal mutation protocol (the §14.8 "Authority mutation
    # journal" machine; B0.1 sheet: "the named internal kernel transitions
    # (activation_admit, resource_allocate, journal mutation protocol)").
    "journal_sql_prepare", "journal_witness_cas", "journal_abandon",
    "journal_sql_finalize",
})

# ------------------------------------------------- C2 governed-work slice ---
# byom_governed_work_v1 slice 1 (C2): the byom-normative binding/enablement/
# formation record shapes under spec/governed-work/ (DESIGN.md §16.3/§16.6;
# family contract §2.A/2.B; plan D10 via kovee amendment A2). Kovee owns the
# host schemas; these are the byom-side normative shapes, so their machines
# are Kovee-owned executors and their descriptors carry owner
# KOVEE_DESCRIPTOR_OWNER (the C2 descriptor ownership rule).
GOVERNED_WORK_SCHEMAS = (
    "kovee-realm-byom-binding", "kovee-society-mapping",
    "kovee-governance-owner-binding", "delegated-principal-credential",
    "endeavor-formation-intent", "endeavor-formation-slot",
    "endeavor-formation-attempt", "kovee-endeavor-form-command",
    "kovee-endeavor-form-arguments", "kovee-endeavor-form-result",
    "external-command-result-query", "external-command-result-query-result",
    "external-command-terminalize-arguments",
    "external-command-terminalize-result",
    "restore-lineage", "restore-lineage-proof",
    # -- slice 2: episode/effect/driver contracts (byom §16.6 items 3-5,
    # 8, 11-12; §7.4; §11.4; §12.1; §14.3; §17.2; family contract Δ4/Δ5,
    # L19-L37, L61-L64) --
    "byom-episode-binding", "byom-subordinate-reservation",
    "provider-context-manifest-byom-fields", "onboarding-compute-intent",
    "onboarding-compute-receipt", "byom-akson-dispatch-arguments",
    "byom-akson-dispatch-outcome-receipt",
    "byom-akson-dispatch-outcome-receipt-head",
    "sender-constrained-worker-credential",
    "sender-constrained-candidate-credential", "act-class-subject",
)
# §16 state/enum lists, transcribed verbatim (order included); the committed
# schema enums must equal them exactly — the machine-checked "states
# verbatim" gate of the C2 sheet. Slice 2 adds the §7.4, §11.4, and §17.2
# lists (an array-valued field's enum lives under items). Derived enums
# (e.g. the DPC/worker sender-constraint methods) are deliberately NOT
# pinned here — only design-verbatim lists are.
GOVERNED_WORK_ENUMS = {
    ("endeavor-formation-intent", "state"): [
        "prepared", "submitting", "remote_unknown", "awaiting_principal",
        "byom_committed", "linking", "linked", "ambiguous", "canceled"],
    ("endeavor-formation-slot", "state"): [
        "held", "submitting", "remote_unknown", "awaiting_principal",
        "byom_committed", "linking", "ambiguous", "released"],
    ("endeavor-formation-attempt", "state"): [
        "prepared", "sent", "reply_received", "transport_unknown",
        "reconciled", "canceled"],
    ("kovee-governance-owner-binding", "governance_owner"): [
        "sage", "byom", "none"],
    ("kovee-governance-owner-binding", "status"): ["active", "frozen"],
    ("kovee-realm-byom-binding", "historical_recovery_mode"): [
        "disabled", "exact_formation_intent_only"],
    ("external-command-result-query-result", "status"): [
        "committed", "absent", "historically_fenced_absent",
        "non_reexecuting_tombstone", "unknown"],
    ("external-command-terminalize-result", "status"): [
        "committed", "terminalized", "not_terminalizable"],
    ("external-command-terminalize-result", "blocking_state"): [
        "prepared_or_in_flight", "lineage_incomplete", "witness_unavailable",
        "domain_conflict"],
    ("restore-lineage", "idempotency_retention"): [
        "complete", "incomplete", "unavailable"],
    ("restore-lineage", "status"): ["current", "superseded"],
    # -- slice 2 --
    ("onboarding-compute-intent", "state"): [
        "prepared", "authorized", "consumed", "completed", "failed",
        "ambiguous"],
    # §7.4 verbatim, `refuse` included as written — the catalog and
    # OnboardingActivationOffer say `membership_refuse` (recorded gap,
    # spec/governed-work/episode-budget-dispatch.md).
    ("onboarding-compute-intent", "allowed_output_operations"): [
        "refuse", "membership_accept", "candidate_self_policy_propose"],
    # §11.4 ExternalBudgetBridge.state verbatim — the bridge-visible saga
    # state carried on the subordinate record (§16.6 item 4).
    ("byom-subordinate-reservation", "state"): [
        "requested", "confirmed", "denied", "uncertain", "settled",
        "released"],
    ("byom-akson-dispatch-outcome-receipt", "disposition"): [
        "pre_result_failed", "ambiguous", "verification_rejected",
        "verified_result"],
    ("byom-akson-dispatch-outcome-receipt", "classification_profile"): [
        "society_mapped_round_trip", "akson_neutral_contract"],
    ("byom-akson-dispatch-outcome-receipt", "outcome"): [
        "succeeded", "failed", "ambiguous"],
    ("byom-akson-dispatch-outcome-receipt", "failure_stage"): [
        "before_dispatch", "stage_rejected", "consent_rejected",
        "dispatch_definitively_rejected", "reconciled_no_result"],
    ("byom-akson-dispatch-outcome-receipt", "ambiguity_stage"): [
        "dispatch_unknown", "result_unknown", "verification_unknown"],
    ("byom-akson-dispatch-outcome-receipt", "verification_rejection_class"): [
        "signature_invalid", "identity_epoch_mismatch", "schema_invalid",
        "digest_mismatch", "evidence_invalid", "contract_mismatch"],
    ("byom-akson-dispatch-outcome-receipt-head", "state"): [
        "ambiguous", "final"],
    # §7.4 OnboardingActivationOffer.allowed_operations verbatim (= the R11
    # candidate set; C3a binds the same three).
    ("sender-constrained-candidate-credential", "allowed_operations"): [
        "membership_refuse", "membership_accept",
        "candidate_self_policy_propose"],
    # Family contract §4 Δ4 verbatim: the closed act-class list.
    ("act-class-subject", "act_class"): [
        "model_egress", "share", "outbound", "apply", "budget"],
}
# C2 descriptors: file stem -> machine name. All are Kovee-owned executor
# machines over byom-normative shapes.
GOVERNED_WORK_DESCRIPTORS = {
    "greenfield-enablement": "GreenfieldEnablement",
    "endeavor-formation": "EndeavorFormationIntent/Slot",
    "byom-episode-binding": "ByomEpisodeBinding",
    "subordinate-reservation": "ByomSubordinateReservation",
    "byom-akson-dispatch-outcome-head": "ByomAksonDispatchOutcomeReceiptHead",
}
# Descriptors whose state list must equal a pinned enum verbatim (the
# formation intent list is §16.3; the subordinate list is the §11.4 bridge
# state list; the dispatch head list is §17.2).
GOVERNED_WORK_DESCRIPTOR_STATES = {
    "endeavor-formation": ("endeavor-formation-intent", "state"),
    "subordinate-reservation": ("byom-subordinate-reservation", "state"),
    "byom-akson-dispatch-outcome-head":
        ("byom-akson-dispatch-outcome-receipt-head", "state"),
}
KOVEE_DESCRIPTOR_OWNER = "kovee (C2)"
# Δ4 act-class subject taxonomy (family contract §4, delivered in C2): the
# mandatory BPA-1 request domains per act class, transcribed from the
# committed act-class-subject arms — check_governed_work verifies the
# schema encodes exactly this table, and the schema's copied BPA-1 $defs
# are byte-identical (JCS) to spec/schemas/bpa1-policy.schema.json's.
ACT_CLASS_MANDATORY = {
    "model_egress": ["operation", "purpose", "binding", "classification",
                     "quantity"],
    "share": ["operation", "purpose", "object", "classification"],
    "outbound": ["operation", "purpose", "network_destination",
                 "classification"],
    "apply": ["operation", "purpose", "object", "path", "schema_evidence"],
    "budget": ["operation", "purpose", "quantity"],
}

# ------------------------------------------------------ C3a MCP bundle ------
# The C3a MCP tool op lists, transcribed verbatim from plan/sheets/C3a.md
# (closed — these exact lists, nothing else; candidate profile normative via
# byom amendment A4). The document mcp/byom-mcp.tools.json must bind exactly
# these and check_mcp_tools fails on any drift.
C3A_CANDIDATE_OPS = ("membership_refuse", "membership_accept",
                     "candidate_self_policy_propose")
C3A_PARTICIPANT_OPS = (
    "activity_open", "activity_show", "activity_hold", "activity_close",
    "wake_intent_submit", "wake_intent_withdraw", "episode_request",
    "continuation_write", "endeavor_propose", "endeavor_position",
    "endeavor_finalize", "call_open", "call_withdraw", "pledge_propose",
    "pledge_position", "pledge_finalize", "pledge_amend", "pledge_resume",
    "pledge_relinquish", "delivery_submit", "delivery_withdraw",
    "mandate_prepare", "mandate_position", "act_intent_prepare",
    "act_intent_position", "act_intent_cancel", "engram_propose",
    "engram_read", "engram_search", "participant_show", "activity_show",
    "society_show", "budget_show", "snapshot_get", "events_read",
    "events_wait", "event_payload", "idempotency_result", "cursor_recover",
)
# The sheet lists activity_show twice (once among the activity ops, once
# among the projection reads); one tool binds each unique op, in
# first-occurrence sheet order.
C3A_PARTICIPANT_UNIQUE = tuple(dict.fromkeys(C3A_PARTICIPANT_OPS))
# Reads (safe_to_allow) within the participant list: SLICE_READS members
# plus the knowledge/budget projection reads whose op schemas land with
# their own bundles (engram_read/engram_search are R4/R37 reads per
# amendment A5; budget_show is a projection read).
C3A_PARTICIPANT_READS = frozenset({
    "activity_show", "participant_show", "society_show", "budget_show",
    "snapshot_get", "events_read", "events_wait", "event_payload",
    "idempotency_result", "cursor_recover", "engram_read", "engram_search",
})
# The channel/bridge envelope the byom-mcp bridge derives (protocol version
# from negotiation, MutationMeta from channel state): never tool args.
MCP_ENVELOPE_FIELDS = frozenset({"version", "op", "meta"})
# Channel-derived fields (gap notes G16/G18/G24/G26/G34/G40): supplied by
# the sender-constrained credential, never by the caller. participant_ref
# is NOT listed — it is a legitimate lookup arg on participant_show; on
# every mutation it is absent from the request schema, so the subset rule
# already rejects it.
MCP_CHANNEL_DERIVED = frozenset({
    "candidate_participant_ref", "candidate_binding_epoch",
    "candidate_actor_ref", "onboarding_fence_epoch", "refused_by_actor_ref",
    "accepted_by_actor_ref", "authentication_observation_ref",
    "actor_ref", "participant_binding_epoch", "endpoint_incarnation",
    "recovery_epoch", "delivered_by_participant", "requested_by_participant",
})


# ---------------------------------------------------------------- I-JSON ----

def _reject_dup_pairs(pairs):
    out = {}
    for key, value in pairs:
        if key in out:
            raise ValueError(f"duplicate object key: {key!r}")
        out[key] = value
    return out


def _check_numbers(value):
    if isinstance(value, bool):
        return
    if isinstance(value, int) and abs(value) > SAFE_MAX:
        raise ValueError(f"unsafe integer: {value}")
    if isinstance(value, list):
        for item in value:
            _check_numbers(item)
    if isinstance(value, dict):
        for item in value.values():
            _check_numbers(item)


def strict_parse(text: str):
    """Strict parsing for the repository's own spec files (schemas,
    descriptors, vector files): duplicate keys, non-finite numbers, and
    unsafe integers fail closed. Wire-body acceptance is `ijson_class`."""
    def _const(name):
        raise ValueError(f"non-finite number: {name}")

    value = json.loads(
        text, object_pairs_hook=_reject_dup_pairs, parse_constant=_const
    )
    _check_numbers(value)
    return value


# ------------------------------------- C1 wire-body acceptance (BYOM-03) ----
# The family acceptance rules of family-vectors/PROFILE.md section 1,
# self-contained here per the runner's convention (the independent family
# rederiver family-vectors/xcheck.py implements the same profile).


class IJsonError(Exception):
    def __init__(self, cls: str):
        super().__init__(cls)
        self.cls = cls


def _scan_one_json_text(text: str):
    """Single-pass validating token scanner for exactly one strict JSON text.

    Iterative (explicit container stack, no recursion), so pathological
    nesting inside the size cap can never raise RecursionError; the stack
    length is the container depth. Values are never materialized -- the
    scanner counts nodes, tracks maximum depth, records decoded-string
    surrogate health, and raises the order-3 error classes of PROFILE.md
    section 1 in token order: `syntax`, `trailing-data`, `duplicate`,
    `reserved-domain-collision`, `unsafe-integer`, `non-finite`,
    `unsafe-number`. Returns (nodes, max_depth, lone_surrogate).
    """
    pos = 0
    n = len(text)
    nodes = 0
    max_depth = 0
    lone_surrogate = False
    stack: list = []  # per-container: a key set for objects, None for arrays

    def syntax():
        raise IJsonError("syntax")

    def digit(i: int) -> bool:
        return i < n and "0" <= text[i] <= "9"

    def skip_ws():
        nonlocal pos
        while pos < n and text[pos] in " \t\n\r":
            pos += 1

    def scan_string() -> str:
        """Scan a string token at `pos` (opening quote), decoding escapes."""
        nonlocal pos, lone_surrogate
        pos += 1  # opening quote
        out: list[str] = []
        while True:
            if pos >= n:
                syntax()
            ch = text[pos]
            if ch == '"':
                pos += 1
                break
            if ch == "\\":
                pos += 1
                if pos >= n:
                    syntax()
                e = text[pos]
                if e in '"\\/':
                    out.append(e)
                elif e == "b":
                    out.append("\b")
                elif e == "f":
                    out.append("\f")
                elif e == "n":
                    out.append("\n")
                elif e == "r":
                    out.append("\r")
                elif e == "t":
                    out.append("\t")
                elif e == "u":
                    hex4 = text[pos + 1 : pos + 5]
                    if len(hex4) != 4 or any(
                        c not in "0123456789abcdefABCDEF" for c in hex4
                    ):
                        syntax()
                    out.append(chr(int(hex4, 16)))
                    pos += 4
                else:
                    syntax()
                pos += 1
            elif ord(ch) < 0x20:
                syntax()  # raw control character in a string
            else:
                out.append(ch)
                pos += 1
        s = "".join(out)
        # Surrogate health after escape decoding (raw text is already valid
        # UTF-8, so unpaired halves can only arrive via \uXXXX escapes). The
        # profile reports this as its own ordered check (order 4), so only a
        # flag is recorded here.
        i = 0
        while i < len(s):
            u = ord(s[i])
            if 0xD800 <= u <= 0xDBFF:
                if i + 1 < len(s) and 0xDC00 <= ord(s[i + 1]) <= 0xDFFF:
                    i += 1
                else:
                    lone_surrogate = True
            elif 0xDC00 <= u <= 0xDFFF:
                lone_surrogate = True
            i += 1
        return s

    def scan_number():
        """Scan a number token at `pos` ('-' or digit) and classify it."""
        nonlocal pos
        start = pos
        if text[pos] == "-":
            pos += 1
            # json's -Infinity spelling is the non-finite class, not syntax
            if text.startswith("Infinity", pos):
                raise IJsonError("non-finite")
        if pos < n and text[pos] == "0":
            pos += 1
        elif digit(pos):
            while digit(pos):
                pos += 1
        else:
            syntax()
        is_float = False
        if pos < n and text[pos] == ".":
            is_float = True
            pos += 1
            if not digit(pos):
                syntax()
            while digit(pos):
                pos += 1
        if pos < n and text[pos] in "eE":
            is_float = True
            pos += 1
            if pos < n and text[pos] in "+-":
                pos += 1
            if not digit(pos):
                syntax()
            while digit(pos):
                pos += 1
        token = text[start:pos]
        if not is_float:
            # Exact magnitude check on the token, immune to double rounding.
            if abs(int(token)) > SAFE_MAX:
                raise IJsonError("unsafe-integer")
        else:
            v = float(token)
            if v != v or v in (float("inf"), float("-inf")):
                raise IJsonError("unsafe-number")
            if v.is_integer() and abs(v) > SAFE_MAX:
                raise IJsonError("unsafe-number")

    VALUE = 0            # a value is required
    VALUE_OR_CLOSE = 1   # just after '[': a value or ']'
    KEY_OR_CLOSE = 2     # just after '{': a key or '}'
    KEY = 3              # after ',' in an object: a key
    COLON = 4
    COMMA_OR_CLOSE = 5   # after a completed member/element
    state = VALUE
    done = False

    def bump_depth():
        nonlocal max_depth
        if len(stack) > max_depth:
            max_depth = len(stack)

    while not done:
        skip_ws()
        if pos >= n:
            syntax()
        ch = text[pos]
        if state in (VALUE, VALUE_OR_CLOSE):
            if state == VALUE_OR_CLOSE and ch == "]":
                pos += 1
                stack.pop()
                if not stack:
                    done = True
                else:
                    state = COMMA_OR_CLOSE
                continue
            if ch == "{":
                pos += 1
                nodes += 1
                stack.append(set())
                bump_depth()
                state = KEY_OR_CLOSE
            elif ch == "[":
                pos += 1
                nodes += 1
                stack.append(None)
                bump_depth()
                state = VALUE_OR_CLOSE
            elif ch == '"':
                scan_string()
                nodes += 1
                done, state = (True, state) if not stack else (False, COMMA_OR_CLOSE)
            elif ch == "-" or digit(pos):
                scan_number()
                nodes += 1
                done, state = (True, state) if not stack else (False, COMMA_OR_CLOSE)
            elif text.startswith("true", pos):
                pos += 4
                nodes += 1
                done, state = (True, state) if not stack else (False, COMMA_OR_CLOSE)
            elif text.startswith("false", pos):
                pos += 5
                nodes += 1
                done, state = (True, state) if not stack else (False, COMMA_OR_CLOSE)
            elif text.startswith("null", pos):
                pos += 4
                nodes += 1
                done, state = (True, state) if not stack else (False, COMMA_OR_CLOSE)
            elif text.startswith("NaN", pos) or text.startswith("Infinity", pos):
                raise IJsonError("non-finite")
            else:
                syntax()
        elif state in (KEY_OR_CLOSE, KEY):
            if state == KEY_OR_CLOSE and ch == "}":
                pos += 1
                stack.pop()
                if not stack:
                    done = True
                else:
                    state = COMMA_OR_CLOSE
                continue
            if ch != '"':
                syntax()
            # Member names in token order: the reserved-name check precedes
            # the duplicate check for the same token; names compare after
            # escape decoding (RFC 7493).
            key = scan_string()
            if key == "$domain":
                raise IJsonError("reserved-domain-collision")
            keys = stack[-1]
            if key in keys:
                raise IJsonError("duplicate")
            keys.add(key)
            state = COLON
        elif state == COLON:
            if ch != ":":
                syntax()
            pos += 1
            state = VALUE
        else:  # COMMA_OR_CLOSE
            top_keys = stack[-1]
            if ch == ",":
                pos += 1
                state = KEY if top_keys is not None else VALUE
            elif ch == ("}" if top_keys is not None else "]"):
                pos += 1
                stack.pop()
                if not stack:
                    done = True
                else:
                    state = COMMA_OR_CLOSE
            else:
                syntax()
    skip_ws()
    if pos < n:
        raise IJsonError("trailing-data")  # exactly one JSON text
    return nodes, max_depth, lone_surrogate


def ijson_class(data: bytes, context: str = "request"):
    """Returns None when `data` is an acceptable strict-I-JSON body for the
    given context ("request": 256 KiB cap; "response": 1 MiB cap), else the
    profile error class. Check order (PROFILE.md section 1): size, UTF-8,
    token scan (syntax / trailing-data / duplicates / reserved `$domain` /
    numeric caps / non-finite), surrogates, depth, node count."""
    cap = RESPONSE_CAP if context == "response" else REQUEST_CAP
    if len(data) > cap:
        return "oversize"
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return "invalid-utf8"
    try:
        nodes, max_depth, lone_surrogate = _scan_one_json_text(text)
    except IJsonError as e:
        return e.cls
    if lone_surrogate:
        return "unpaired-surrogate"
    if max_depth > DEPTH_CAP:
        return "over-depth"
    if nodes > NODE_CAP:
        return "over-nodes"
    return None


# ------------------------------------------------------------------- JCS ----

_ESC = {
    0x08: "\\b", 0x09: "\\t", 0x0A: "\\n", 0x0C: "\\f", 0x0D: "\\r",
    0x22: '\\"', 0x5C: "\\\\",
}


def _jcs_string(s: str) -> str:
    out = ['"']
    for ch in s:
        cp = ord(ch)
        if cp in _ESC:
            out.append(_ESC[cp])
        elif cp < 0x20:
            out.append("\\u%04x" % cp)
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def _es_number(v: float) -> str:
    """ECMAScript Number::toString(10) for a finite double (RFC 8785
    3.2.2.3). Python's repr() already yields the shortest round-trip decimal
    digits (same digits as ES); only the layout rules differ, applied here.
    Ported from the family rederiver approach (R0/BYOM-03): the profile JCS
    covers the full finite-float value space section 1 admits."""
    if v != v or v in (float("inf"), float("-inf")):
        raise ValueError("non-finite number in JCS input")
    if v == 0.0:
        return "0"  # covers -0.0, as in ES
    sign = "-" if v < 0 else ""
    r = repr(abs(v))
    if "e" in r:
        mant, _, exp_s = r.partition("e")
        exp = int(exp_s)
    else:
        mant, exp = r, 0
    ip, _, fp = mant.partition(".")
    digits = (ip + fp).lstrip("0")
    stripped = digits.rstrip("0")
    trailing = len(digits) - len(stripped)
    k = len(stripped)
    n = k + trailing + exp - len(fp)  # value == 0.<stripped> * 10**n
    s = stripped
    if k <= n <= 21:
        out = s + "0" * (n - k)
    elif 0 < n <= 21:
        out = s[:n] + "." + s[n:]
    elif -6 < n <= 0:
        out = "0." + "0" * (-n) + s
    else:
        e = n - 1
        out = (s[0] + ("." + s[1:] if k > 1 else "")
               + "e" + ("+" if e >= 0 else "-") + str(abs(e)))
    return sign + out


def jcs(value) -> str:
    """RFC 8785 JCS over the profile value space (PROFILE.md section 2): the
    full strict-I-JSON space section 1 admits, including finite floats in ES
    minimal number form. BPP canonical values happen to contain no floats
    today (§14.2, ADR-0001), but the canonicalizer implements the family
    profile, not that narrower habit (R0/BYOM-03)."""
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int):
        if abs(value) > SAFE_MAX:
            raise ValueError("unsafe integer")
        return str(value)
    if isinstance(value, float):
        return _es_number(value)
    if isinstance(value, str):
        return _jcs_string(value)
    if isinstance(value, list):
        return "[" + ",".join(jcs(v) for v in value) + "]"
    if isinstance(value, dict):
        items = sorted(value.items(), key=lambda kv: kv[0].encode("utf-16-be"))
        return "{" + ",".join(
            _jcs_string(k) + ":" + jcs(v) for k, v in items
        ) + "}"
    raise TypeError(f"unsupported type: {type(value)}")


def tagged_jcs(domain: str, value) -> str:
    """Byom type-tagged canonical bytes (PROFILE.md section 2, normative;
    supersedes the B0.1 JCS([domain, value]) proposal — R0/BYOM-01): inject
    the reserved `$domain` member at the top level, then JCS. Fails closed if
    the object already carries `$domain`."""
    if not isinstance(value, dict):
        raise ValueError("type-tagged canonicalization requires an object")
    if "$domain" in value:
        raise ValueError("object already carries a $domain member")
    return jcs({**value, "$domain": domain})


def hmac_sha256_hex(secret_hex: str, canonical: str) -> str:
    return hmac.new(
        bytes.fromhex(secret_hex), canonical.encode("utf-8"), hashlib.sha256
    ).hexdigest()


# ------------------------------------------------- schema conventions -------

PROBLEM_TYPE_PREFIX = "https://byom.dev/problems/"


def _restore_lineage_proof_ok(value) -> bool:
    """The declared hop_count must equal the ordered_hops length
    (DESIGN.md §16.3); JSON Schema cannot compare a member against an array
    length, so the runner enforces it (the bpp-failure type/kind pattern).
    Applied only where both members carry their schema-checked shapes."""
    if not isinstance(value, dict):
        return True
    count, hops = value.get("hop_count"), value.get("ordered_hops")
    if isinstance(count, int) and not isinstance(count, bool) \
            and isinstance(hops, list):
        return count == len(hops)
    return True


def _subordinate_reservation_ok(value) -> bool:
    """Never-above-parent (DESIGN.md §11.4; family contract L32): every
    byom_subordinate item's amount must be <= its parent item's
    worst_case_amount, with the SAME dimension and unit — a subordinate
    reservation may narrow or deny but never reshape or parallel-charge.
    JSON Schema cannot compare two members, so the runner enforces it (the
    restore-lineage-proof pattern). Applied only where the members carry
    their schema-checked shapes."""
    if not isinstance(value, dict) or not isinstance(value.get("items"),
                                                     list):
        return True
    for item in value["items"]:
        if not isinstance(item, dict):
            continue
        amount = item.get("amount")
        cap = item.get("parent_worst_case_amount")
        if isinstance(amount, int) and isinstance(cap, int) \
                and not isinstance(amount, bool) \
                and not isinstance(cap, bool) and amount > cap:
            return False
        for member, parent in (("dimension", "parent_dimension"),
                               ("unit", "parent_unit")):
            a, b = item.get(member), item.get(parent)
            if isinstance(a, str) and isinstance(b, str) and a != b:
                return False
    return True


def _failure_type_kind_ok(envelope) -> bool:
    """Problem type must equal exactly PROBLEM_TYPE_PREFIX + kind (PROFILE.md
    §3, profile-pinned decision 3). Applied only where both members are
    present strings; their presence and shape are the schema's job."""
    problem = envelope.get("problem") if isinstance(envelope, dict) else None
    if not isinstance(problem, dict):
        return True
    kind, typ = problem.get("kind"), problem.get("type")
    if isinstance(kind, str) and isinstance(typ, str):
        return typ == PROBLEM_TYPE_PREFIX + kind
    return True


def _walk_dicts(node):
    if isinstance(node, dict):
        yield node
        for v in node.values():
            yield from _walk_dicts(v)
    elif isinstance(node, list):
        for v in node:
            yield from _walk_dicts(v)


def _resolve_pointer(root, ref: str):
    if not ref.startswith("#"):
        raise KeyError(ref)
    node = root
    pointer = ref[1:]
    for part in [p for p in pointer.split("/") if p]:
        part = part.replace("~1", "/").replace("~0", "~")
        node = node[part]
    return node


def convention_errors(schema: dict) -> list[str]:
    errs = []
    if schema.get("$schema") != DRAFT:
        errs.append(f"$schema must be {DRAFT}")
    if not schema.get("$id"):
        errs.append("$id is required")
    for node in _walk_dicts(schema):
        ref = node.get("$ref")
        if isinstance(ref, str):
            if not ref.startswith("#"):
                errs.append(f"remote $ref forbidden: {ref}")
            else:
                try:
                    _resolve_pointer(schema, ref)
                except KeyError:
                    errs.append(f"unresolvable $ref: {ref}")
        if isinstance(node.get("properties"), dict):
            # Closed schemas (spec/README.md conventions): a defining object
            # schema (one that declares type) must either close itself with
            # additionalProperties false or constrain every member name with
            # a propertyNames pattern (the RFC 9457 problem-extension case,
            # R0/BYOM-02). Refinement branches without `type` (oneOf arms)
            # constrain members of an already-closed parent and are exempt.
            closed = node.get("additionalProperties") is False
            named_extensions = isinstance(node.get("propertyNames"), dict)
            refinement = "type" not in node
            if not (closed or named_extensions or refinement):
                errs.append(
                    "object schema with properties must set "
                    "additionalProperties false or constrain names via "
                    f"propertyNames (near {sorted(node['properties'])[:3]})"
                )
        pattern = node.get("pattern")
        if isinstance(pattern, str):
            try:
                re.compile(pattern)
            except re.error as exc:
                errs.append(f"invalid pattern {pattern!r}: {exc}")
    return errs


# ---------------------------------------------------- minimal validator -----

def _is_type(instance, name: str) -> bool:
    if name == "object":
        return isinstance(instance, dict)
    if name == "array":
        return isinstance(instance, list)
    if name == "string":
        return isinstance(instance, str)
    if name == "boolean":
        return isinstance(instance, bool)
    if name == "null":
        return instance is None
    if name == "integer":
        if isinstance(instance, bool):
            return False
        return isinstance(instance, int) or (
            isinstance(instance, float) and instance.is_integer()
        )
    if name == "number":
        return not isinstance(instance, bool) and isinstance(instance, (int, float))
    return False


def _equal(a, b) -> bool:
    if isinstance(a, bool) != isinstance(b, bool):
        return False
    return a == b


def mini_valid(root: dict, schema, instance) -> bool:
    """Just enough of draft 2020-12 for the keyword set these schemas use:
    boolean schemas, $ref (internal), type, const, enum, pattern, min/max
    Length, minimum/maximum, required, properties, additionalProperties,
    propertyNames, oneOf, not, items, minItems, maxItems, uniqueItems."""
    if schema is True:
        return True
    if schema is False:
        return False

    ref = schema.get("$ref")
    if ref is not None:
        try:
            target = _resolve_pointer(root, ref)
        except KeyError:
            return False
        if not mini_valid(root, target, instance):
            return False

    one_of = schema.get("oneOf")
    if one_of is not None:
        matches = sum(1 for sub in one_of if mini_valid(root, sub, instance))
        if matches != 1:
            return False

    if "not" in schema and mini_valid(root, schema["not"], instance):
        return False

    typ = schema.get("type")
    if typ is not None:
        names = typ if isinstance(typ, list) else [typ]
        if not any(_is_type(instance, n) for n in names):
            return False

    if "const" in schema and not _equal(instance, schema["const"]):
        return False
    if "enum" in schema and not any(_equal(instance, e) for e in schema["enum"]):
        return False

    if isinstance(instance, str):
        if "pattern" in schema and not re.search(schema["pattern"], instance):
            return False
        if "minLength" in schema and len(instance) < schema["minLength"]:
            return False
        if "maxLength" in schema and len(instance) > schema["maxLength"]:
            return False

    if isinstance(instance, (int, float)) and not isinstance(instance, bool):
        if "minimum" in schema and instance < schema["minimum"]:
            return False
        if "maximum" in schema and instance > schema["maximum"]:
            return False

    if isinstance(instance, dict):
        for key in schema.get("required", []):
            if key not in instance:
                return False
        prop_names = schema.get("propertyNames")
        if prop_names is not None:
            if not all(mini_valid(root, prop_names, k) for k in instance):
                return False
        props = schema.get("properties", {})
        for key, sub in props.items():
            if key in instance and not mini_valid(root, sub, instance[key]):
                return False
        addl = schema.get("additionalProperties")
        if addl is False:
            if any(k not in props for k in instance):
                return False
        elif isinstance(addl, dict):
            for k, v in instance.items():
                if k not in props and not mini_valid(root, addl, v):
                    return False

    if isinstance(instance, list):
        if "minItems" in schema and len(instance) < schema["minItems"]:
            return False
        if "maxItems" in schema and len(instance) > schema["maxItems"]:
            return False
        if schema.get("uniqueItems"):
            seen = [json.dumps(i, sort_keys=True) for i in instance]
            if len(set(seen)) != len(seen):
                return False
        items = schema.get("items")
        if items is not None:
            if not all(mini_valid(root, items, i) for i in instance):
                return False

    return True


# ---------------------------------------------------------------- runner ----

class Runner:
    def __init__(self, spec_dir: Path):
        self.spec_dir = spec_dir
        self.failures: list[str] = []
        self.schemas: dict[str, dict] = {}
        try:
            import jsonschema  # noqa: F401
            self.jsonschema = jsonschema
        except ImportError:
            self.jsonschema = None

    def fail(self, message: str):
        self.failures.append(message)
        print(f"FAIL  {message}")

    # -- schemas --

    def load_schemas(self) -> int:
        schema_dir = self.spec_dir / "schemas"
        paths = sorted(schema_dir.rglob("*.schema.json"))
        # C2 governed-work record schemas live in their own bundle directory
        # (spec/governed-work/) but share one schema namespace with
        # spec/schemas/ — duplicate names fail below.
        paths += sorted((self.spec_dir / "governed-work")
                        .glob("*.schema.json"))
        if not paths:
            self.fail(f"no schemas found under {schema_dir}")
            return 0
        for path in paths:
            name = path.name.removesuffix(".schema.json")
            if name in self.schemas:
                self.fail(f"{path.name}: duplicate schema name {name!r}")
                continue
            try:
                schema = strict_parse(path.read_text(encoding="utf-8"))
            except ValueError as exc:
                self.fail(f"{path.name}: not strict I-JSON: {exc}")
                continue
            for err in convention_errors(schema):
                self.fail(f"{path.name}: {err}")
            if self.jsonschema is not None:
                try:
                    validator_cls = self.jsonschema.validators.validator_for(schema)
                    validator_cls.check_schema(schema)
                except Exception as exc:
                    self.fail(f"{path.name}: does not compile: {exc}")
                    continue
            self.schemas[name] = schema
        return len(paths)

    def _validate(self, schema_name: str, ref: str | None, value) -> bool:
        schema = self.schemas[schema_name]
        target = schema if ref is None else {"$ref": ref, "$defs": schema["$defs"]}
        if self.jsonschema is not None:
            validator = self.jsonschema.Draft202012Validator(target)
            return validator.is_valid(value)
        if ref is None:
            return mini_valid(schema, schema, value)
        return mini_valid(schema, _resolve_pointer(schema, ref), value)

    # -- bundle op list vs schemas --

    def check_bundle(self) -> int:
        """B0.1 registry-derived rule (spec/README.md bundle-freeze): the
        bundle op list, not prose, decides schema membership. The list is the
        B0.1 sheet transcription above — completeness assertion: every op in
        every sheet family must be a §14.6 catalog operation and must have a
        closed request/result schema pair; the request pins the exact op
        const; mutations require meta; reads carry none."""
        for family, ops in B01_SHEET.items():
            for op in ops:
                if op not in ALL_CATALOG_OPS:
                    self.fail(f"sheet: {family} op {op} is not a §14.6 "
                              "catalog operation (bad transcription or "
                              "catalog drift)")
        covered = 0
        for op in SLICE_OPS:
            base = op.replace("_", "-")
            request = self.schemas.get(f"{base}-request")
            result = self.schemas.get(f"{base}-result")
            ok = True
            if request is None:
                self.fail(f"bundle: op {op} has no {base}-request schema")
                ok = False
            if result is None:
                self.fail(f"bundle: op {op} has no {base}-result schema")
                ok = False
            if request is not None:
                op_const = (request.get("properties", {})
                            .get("op", {}).get("const"))
                if op_const != op:
                    self.fail(f"bundle: {base}-request op const is "
                              f"{op_const!r}, expected {op!r}")
                    ok = False
                required = request.get("required", [])
                has_meta = "meta" in request.get("properties", {})
                if op in SLICE_READS:
                    if has_meta:
                        self.fail(f"bundle: read {op} declares meta "
                                  "(reads never mutate, §14.2)")
                        ok = False
                elif not has_meta or "meta" not in required:
                    self.fail(f"bundle: mutation {op} does not require meta "
                              "(§14.2: every mutation requires request id "
                              "and idempotency key)")
                    ok = False
            if ok:
                covered += 1
        return covered

    # -- C2 governed-work family --

    def check_governed_work(self) -> dict:
        """C2 (byom §16.3/§16.6 plus, for slice 2, §7.4/§11.4/§12.1/§14.3/
        §17.2; family contract §2.A-2.F/2.J and Δ4/Δ5): the closed
        record-schema inventory is present; every design enum equals its
        verbatim transcription (order included; an array field's enum lives
        under items); the Kovee-owned executor descriptors exist, declare
        owner KOVEE_DESCRIPTOR_OWNER and the expected machine name, and the
        pinned descriptors' state lists equal their design lists verbatim;
        and the Δ4 taxonomy schema encodes exactly ACT_CLASS_MANDATORY with
        its BPA-1 $defs byte-identical to bpa1-policy.schema.json (the
        static half of the taxonomy<->BPA-1 cross-validation; the dynamic
        half replays every act-class-subject vector through policy/eval.py
        in _run_schema_vector)."""
        info = {"schemas": 0, "enums": 0, "descriptors": 0, "taxonomy": 0}
        for name in GOVERNED_WORK_SCHEMAS:
            if name not in self.schemas:
                self.fail(f"governed-work: record schema {name} is missing "
                          "from spec/governed-work/")
                continue
            info["schemas"] += 1
        for (name, field), want in GOVERNED_WORK_ENUMS.items():
            schema = self.schemas.get(name)
            if schema is None:
                continue  # already failed above
            prop = schema.get("properties", {}).get(field, {})
            got = prop.get("enum", prop.get("items", {}).get("enum"))
            if got != want:
                self.fail(f"governed-work: {name}.{field} enum is not the "
                          f"design list verbatim\n      schema: {got}\n"
                          f"      design: {want}")
                continue
            info["enums"] += 1
        for stem, machine in GOVERNED_WORK_DESCRIPTORS.items():
            path = self.spec_dir / "descriptors" / f"{stem}.json"
            if not path.is_file():
                self.fail(f"governed-work: descriptor {stem}.json is missing")
                continue
            try:
                body = strict_parse(path.read_text(encoding="utf-8"))
            except ValueError:
                continue  # run_descriptors reports the parse failure
            ok = True
            if body.get("owner") != KOVEE_DESCRIPTOR_OWNER:
                self.fail(f"governed-work: {stem}.json owner is "
                          f"{body.get('owner')!r}, expected "
                          f"{KOVEE_DESCRIPTOR_OWNER!r} (Kovee-owned executor "
                          "over byom-normative shapes)")
                ok = False
            if body.get("machine") != machine:
                self.fail(f"governed-work: {stem}.json machine is "
                          f"{body.get('machine')!r}, expected {machine!r}")
                ok = False
            if stem in GOVERNED_WORK_DESCRIPTOR_STATES:
                want = GOVERNED_WORK_ENUMS[
                    GOVERNED_WORK_DESCRIPTOR_STATES[stem]]
                if body.get("states") != want:
                    self.fail(f"governed-work: {stem}.json states are not "
                              "the design list verbatim\n"
                              f"      descriptor: {body.get('states')}\n"
                              f"      design:     {want}")
                    ok = False
            if ok:
                info["descriptors"] += 1
        info["taxonomy"] = self._check_taxonomy_schema()
        return info

    def _check_taxonomy_schema(self) -> int:
        """Static taxonomy<->BPA-1 agreement (Δ4): the act-class-subject
        oneOf arms encode exactly ACT_CLASS_MANDATORY, and every $def the
        schema copied from bpa1-policy is byte-identical (JCS) — so the
        subject wire IS the BPA-1 request-atoms wire, not a fork of it.
        Returns the number of verified class arms."""
        schema = self.schemas.get("act-class-subject")
        bpa1 = self.schemas.get("bpa1-policy")
        if schema is None or bpa1 is None:
            return 0  # missing schema already failed above
        ok_arms = 0
        arms = {}
        for i, arm in enumerate(schema.get("oneOf", [])):
            cls = (arm.get("properties", {}).get("act_class", {})
                   .get("const"))
            required = (arm.get("properties", {}).get("subject_atoms", {})
                        .get("required"))
            if cls is None or cls in arms:
                self.fail(f"governed-work: act-class-subject oneOf[{i}] has "
                          "no act_class const or repeats one")
                continue
            arms[cls] = required
        if list(arms) != list(ACT_CLASS_MANDATORY):
            self.fail("governed-work: act-class-subject arms are not the Δ4 "
                      f"class list verbatim\n      schema: {list(arms)}\n"
                      f"      Δ4:     {list(ACT_CLASS_MANDATORY)}")
        for cls, want in ACT_CLASS_MANDATORY.items():
            if arms.get(cls) != want:
                self.fail(f"governed-work: act-class-subject {cls} mandatory "
                          f"domains {arms.get(cls)} != transcription {want}")
                continue
            ok_arms += 1
        jcs = lambda v: json.dumps(v, sort_keys=True, separators=(",", ":"))
        for name, body in schema.get("$defs", {}).items():
            source = bpa1.get("$defs", {}).get(name)
            if source is None:
                self.fail(f"governed-work: act-class-subject $defs/{name} "
                          "does not exist in bpa1-policy (the subject wire "
                          "must be the BPA-1 request-atoms wire)")
            elif jcs(body) != jcs(source):
                self.fail(f"governed-work: act-class-subject $defs/{name} "
                          "diverges from bpa1-policy's (byte-identical copy "
                          "required)")
        return ok_arms

    # -- C3a MCP tool bundle --

    def _check_mcp_tool(self, profile: str, tool: dict, info: dict):
        """Per-tool C3a rules: name = byom_<op>; reads safe_to_allow and
        mutations gated (the akson-mcp marking); input-schema fields a
        subset of the committed op request schema's args (envelope
        excluded) — no invented, no channel-derived (G16) fields; required
        args carried faithfully. A tool whose op has no committed request
        schema must declare op_request_schema null and expose zero args."""
        op = tool["op"]
        name = tool["name"]
        if name != f"byom_{op}":
            self.fail(f"c3a: {profile} tool {name} does not equal "
                      f"byom_{op}")
        want_access = ("safe_to_allow" if op in C3A_PARTICIPANT_READS
                       and profile == "participant" else "gated")
        if tool["access"] != want_access:
            self.fail(f"c3a: {name} access is {tool['access']!r}, expected "
                      f"{want_access!r} (reads safe_to_allow, mutations "
                      "gated)")
        input_schema = tool["input_schema"]
        props = set(input_schema.get("properties", {}))
        required = set(input_schema.get("required", []))
        derived = props & MCP_CHANNEL_DERIVED
        if derived:
            self.fail(f"c3a: {name} input schema carries channel-derived "
                      f"field(s) {sorted(derived)} (G16: the credential "
                      "supplies them, never the caller)")
        base = op.replace("_", "-")
        request = self.schemas.get(f"{base}-request")
        if request is None:
            info["pending"].append(op)
            if tool["op_request_schema"] is not None:
                self.fail(f"c3a: {name} names op_request_schema "
                          f"{tool['op_request_schema']!r} but no {base}-"
                          "request schema is committed")
            if props or required:
                self.fail(f"c3a: {name} has no committed {base}-request "
                          "schema — a pending tool must expose zero args")
            return
        if tool["op_request_schema"] != f"{base}-request":
            self.fail(f"c3a: {name} op_request_schema is "
                      f"{tool['op_request_schema']!r}, expected "
                      f"'{base}-request'")
        args = set(request.get("properties", {})) - MCP_ENVELOPE_FIELDS
        invented = props - args
        if invented:
            self.fail(f"c3a: {name} input schema invents field(s) "
                      f"{sorted(invented)} not in {base}-request args")
        req_args = set(request.get("required", [])) - MCP_ENVELOPE_FIELDS
        if required != req_args:
            self.fail(f"c3a: {name} required {sorted(required)} != "
                      f"{base}-request required args {sorted(req_args)}")

    def check_mcp_tools(self) -> dict:
        """C3a check: mcp/byom-mcp.tools.json validates against the closed
        meta-schema; each profile's op list EXACTLY equals the transcribed
        C3a sheet list; per-tool rules via _check_mcp_tool; zero
        governance, runtime, or admin operations bound (deny-by-absence
        made explicit)."""
        info = {"candidate": 0, "participant": 0, "pending": []}
        self._mcp_doc = None
        for op in C3A_CANDIDATE_OPS + C3A_PARTICIPANT_OPS:
            if op not in ALL_CATALOG_OPS:
                self.fail(f"c3a: sheet op {op} is not a §14.6 catalog "
                          "operation (bad transcription or catalog drift)")
        path = Path(__file__).resolve().parent.parent / "mcp" / \
            "byom-mcp.tools.json"
        if "mcp-tools" not in self.schemas:
            self.fail("c3a: meta-schema spec/schemas/mcp-tools.schema.json "
                      "is missing or did not load")
            return info
        if not path.is_file():
            self.fail(f"c3a: tools document not found: {path}")
            return info
        try:
            doc = strict_parse(path.read_text(encoding="utf-8"))
        except ValueError as exc:
            self.fail(f"c3a: byom-mcp.tools.json is not strict I-JSON: {exc}")
            return info
        if not self._validate("mcp-tools", None, doc):
            self.fail("c3a: byom-mcp.tools.json does not validate against "
                      "the mcp-tools meta-schema")
            return info
        self._mcp_doc = doc
        expected = {"candidate": C3A_CANDIDATE_OPS,
                    "participant": C3A_PARTICIPANT_UNIQUE}
        for profile, want in expected.items():
            tools = doc["profiles"][profile]["tools"]
            got = tuple(t["op"] for t in tools)
            if got != want:
                self.fail(f"c3a: {profile} profile op list != C3a sheet "
                          f"list\n      bound: {got}\n      sheet: {want}")
                continue
            for tool in tools:
                self._check_mcp_tool(profile, tool, info)
            info[profile] = len(tools)
        bound = {t["op"] for env in doc["profiles"].values()
                 for t in env["tools"]}
        allowed = set(C3A_CANDIDATE_OPS) | set(C3A_PARTICIPANT_OPS)
        for surface, ops in (("admin", CATALOG["administration"]),
                             ("runtime", CATALOG["runtime"]
                              + CATALOG["host_integration"])):
            hits = bound & set(ops)
            if hits:
                self.fail(f"c3a: {surface} operation(s) bound as tools: "
                          f"{sorted(hits)} — 'no governance, runtime, or "
                          "admin operation, ever'")
        stray = bound - allowed
        if stray:
            self.fail("c3a: op(s) outside the closed C3a lists (governance "
                      f"or other surface): {sorted(stray)}")
        return info

    # -- transition descriptors --

    def _descriptor_shape_errors(self, body) -> list[str]:
        errs = []
        keys = set(body) if isinstance(body, dict) else set()
        if not isinstance(body, dict) or not (
                {"machine", "states", "transitions"} <= keys
                and keys <= {"machine", "states", "transitions", "owner"}):
            return ["top-level keys must be exactly "
                    "{machine, states, transitions} plus optional owner"]
        if "owner" in body and not (isinstance(body["owner"], str)
                                    and body["owner"]):
            errs.append("owner, when present, must be a non-empty string")
        if not (isinstance(body["machine"], str) and body["machine"]):
            errs.append("machine must be a non-empty string")
        states = body["states"]
        if (not isinstance(states, list) or not states
                or not all(isinstance(s, str) and s for s in states)):
            errs.append("states must be a non-empty list of state names")
            states = []
        if len(set(states)) != len(states):
            errs.append("duplicate state names")
        if "absent" in states:
            errs.append("'absent' is the implicit pre-creation state and "
                        "must not be listed")
        allowed_from = set(states) | {"absent"}
        transitions = body["transitions"]
        if not isinstance(transitions, list) or not transitions:
            return errs + ["transitions must be a non-empty list"]
        for i, row in enumerate(transitions):
            where = f"transitions[{i}]"
            if not isinstance(row, dict):
                errs.append(f"{where}: not an object")
                continue
            missing = {"from", "to", "via", "authority"} - set(row)
            extra = set(row) - {"from", "to", "via", "authority", "notes",
                                "cascade"}
            if missing:
                errs.append(f"{where}: missing {sorted(missing)}")
            if extra:
                errs.append(f"{where}: unknown keys {sorted(extra)}")
            if row.get("from") not in allowed_from:
                errs.append(f"{where}: from {row.get('from')!r} is not a "
                            "declared state or 'absent'")
            if row.get("to") not in set(states):
                errs.append(f"{where}: to {row.get('to')!r} is not a "
                            "declared state")
            for key in ("via", "authority"):
                if not (isinstance(row.get(key), str) and row.get(key)):
                    errs.append(f"{where}: {key} must be a non-empty string")
            if "notes" in row and not (isinstance(row["notes"], str)
                                       and row["notes"]):
                errs.append(f"{where}: notes must be a non-empty string")
            if "cascade" in row and row["cascade"] is not True:
                errs.append(f"{where}: cascade, when present, must be true")
        return errs

    def run_descriptors(self) -> dict:
        """§14.8 one-to-one rule for this slice: every mutating operation
        appears in exactly one descriptor's owning transitions. Where §14.8
        repeats an operation across machine rows (refusal/revocation/
        admission cascades), the non-owning occurrences carry cascade: true
        and must cite an operation owned by a different descriptor (gap
        note G13 in spec/schemas/ops/README.md)."""
        desc_dir = self.spec_dir / "descriptors"
        counts = {"files": 0, "states": 0, "transitions": 0, "owned": 0,
                  "kovee": 0}
        paths = sorted(desc_dir.glob("*.json"))
        if not paths:
            self.fail(f"no descriptors found under {desc_dir}")
            return counts
        machines: dict[str, str] = {}
        owners: dict[str, set[str]] = {}
        cascades: list[tuple[str, str]] = []
        for path in paths:
            name = path.name
            try:
                body = strict_parse(path.read_text(encoding="utf-8"))
            except ValueError as exc:
                self.fail(f"{name}: descriptor is not strict I-JSON: {exc}")
                continue
            errs = self._descriptor_shape_errors(body)
            for err in errs:
                self.fail(f"{name}: {err}")
            if errs:
                continue
            machine = body["machine"]
            if machine in machines:
                self.fail(f"{name}: machine {machine!r} already described "
                          f"by {machines[machine]}")
            machines[machine] = name
            counts["files"] += 1
            counts["states"] += len(body["states"])
            counts["transitions"] += len(body["transitions"])
            # C2 descriptor ownership rule: a descriptor declaring a Kovee
            # owner describes a Kovee-owned executor machine over
            # byom-normative shapes (§16.3/§16.6). Its vias are Kovee-side
            # transition names or host-integration operations; they never own
            # a BPP operation, so they stay out of the exactly-once owners
            # map below. Reads still cannot drive transitions.
            kovee_owned = isinstance(body.get("owner"), str) \
                and body["owner"].startswith("kovee")
            if kovee_owned:
                counts["kovee"] += 1
            for row in body["transitions"]:
                via = row["via"]
                if via in SLICE_READS:
                    self.fail(f"{name}: read operation {via!r} cannot drive "
                              "a transition (reads never mutate, §14.2)")
                    continue
                if kovee_owned:
                    continue
                if via not in ALL_CATALOG_OPS and via not in NAMED_TRANSITIONS:
                    self.fail(f"{name}: via {via!r} is neither a §14.6 "
                              "catalog operation nor a named kernel/server "
                              "transition")
                    continue
                if row.get("cascade"):
                    if via not in ALL_CATALOG_OPS:
                        self.fail(f"{name}: cascade via {via!r} must be an "
                                  "operation, not a named transition")
                    cascades.append((name, via))
                elif via in ALL_CATALOG_OPS:
                    owners.setdefault(via, set()).add(name)
        for op in SLICE_MUTATING:
            files = sorted(owners.get(op, ()))
            if len(files) == 1:
                counts["owned"] += 1
            elif not files:
                self.fail(f"descriptor parity: mutating op {op} appears in "
                          "no descriptor's owning transitions")
            else:
                self.fail(f"descriptor parity: mutating op {op} owned by "
                          f"multiple descriptors: {files}")
        for name, via in cascades:
            if via not in SLICE_MUTATING:
                continue  # other-family op; its owner lands with its slice
            owning = owners.get(via, set())
            if name in owning:
                self.fail(f"{name}: cascade via {via!r} cannot cascade "
                          "inside its own owning descriptor")
            elif not owning:
                self.fail(f"{name}: cascade via {via!r} has no owning "
                          "descriptor")
        return counts

    # -- vectors --

    def run_vectors(self) -> dict:
        vector_dir = self.spec_dir / "vectors"
        counts = {"schema-valid": 0, "schema-invalid": 0, "acceptance": 0,
                  "digest": 0, "machine-walk": 0, "policy": 0,
                  "tool-call": 0, "taxonomy-bpa1": 0}
        paths = sorted(p for p in vector_dir.rglob("*.json"))
        if not paths:
            self.fail(f"no vectors found under {vector_dir}")
        for path in paths:
            rel = path.relative_to(vector_dir)
            try:
                vector = strict_parse(path.read_text(encoding="utf-8"))
            except ValueError as exc:
                self.fail(f"{rel}: vector file is not strict I-JSON: {exc}")
                continue
            expected_name = rel.with_suffix("").as_posix()
            if vector.get("name") != expected_name:
                self.fail(f"{rel}: name {vector.get('name')!r} != {expected_name!r}")
            inp = vector.get("input", {})
            expected = vector.get("expected", {})
            if "schema" in inp:
                self._run_schema_vector(rel, inp, expected, counts)
            elif "raw" in inp or "raw_base64" in inp or "json_synth" in inp:
                self._run_acceptance_vector(rel, inp, expected, counts)
            elif "policy_op" in inp:
                self._run_policy_vector(rel, inp, expected, counts)
            elif "tool_call" in inp:
                self._run_tool_call_vector(rel, inp, expected, counts)
            elif "domain" in inp:
                self._run_digest_vector(rel, inp, expected, counts)
            elif "machine" in inp:
                self._run_machine_vector(rel, inp, expected, counts)
            else:
                self.fail(f"{rel}: unknown vector kind (input keys {sorted(inp)})")
        return counts

    def _run_schema_vector(self, rel, inp, expected, counts):
        schema_name = inp["schema"]
        if schema_name not in self.schemas:
            self.fail(f"{rel}: references unknown schema {schema_name!r}")
            return
        verdict = self._validate(schema_name, inp.get("ref"), inp["value"])
        if verdict and schema_name == "bpp-failure" and inp.get("ref") is None:
            # Exact type/kind agreement (PROFILE.md §3): JSON Schema cannot
            # cross-reference two members, so the schema's $comment defers to
            # this convention check (R0/BYOM-02).
            verdict = _failure_type_kind_ok(inp["value"])
        if verdict and schema_name == "restore-lineage-proof" \
                and inp.get("ref") is None:
            # §16.3: declared hop count MUST equal the array length.
            verdict = _restore_lineage_proof_ok(inp["value"])
        if verdict and schema_name == "byom-subordinate-reservation" \
                and inp.get("ref") is None:
            # §11.4: never above (or reshaping) the parent dimension.
            verdict = _subordinate_reservation_ok(inp["value"])
        if schema_name == "act-class-subject" and inp.get("ref") is None:
            # Dynamic taxonomy<->BPA-1 cross-validation (Δ4): the subject
            # atoms must decide through the BPA-1 reference evaluator
            # policy/eval.py (a universal allow policy — decide() first
            # validates the request wire). A schema-valid subject the
            # evaluator rejects is a divergence between the two encodings
            # and fails hard, beyond this vector's verdict.
            eval_ok = self._taxonomy_bpa1_ok(inp["value"])
            counts["taxonomy-bpa1"] += 1
            if verdict and not eval_ok:
                self.fail(f"{rel}: taxonomy<->BPA-1 divergence — the "
                          "subject validates against act-class-subject but "
                          "policy/eval.py rejects its atoms")
            verdict = verdict and eval_ok
        if verdict != expected["valid"]:
            self.fail(f"{rel}: expected valid={expected['valid']}, got {verdict}")
            return
        counts["schema-valid" if expected["valid"] else "schema-invalid"] += 1

    def _taxonomy_bpa1_ok(self, value) -> bool:
        """True when the subject's atoms are a well-formed BPA-1 request
        the reference evaluator decides (allow under a universal allow
        policy); False on any typed rejection or a non-object subject."""
        atoms = value.get("subject_atoms") if isinstance(value, dict) \
            else None
        if not isinstance(atoms, dict):
            return False
        mod = self._policy_eval()
        result = mod.run_case({
            "policy_op": "decide",
            "policy": {"rules": [{"effect": "allow", "atoms": {}}]},
            "request": atoms,
        })
        return result.get("ok") is True and result.get("decision") == "allow"

    def _run_acceptance_vector(self, rel, inp, expected, counts):
        if "raw" in inp:
            raw = inp["raw"].encode("utf-8")
        elif "raw_base64" in inp:
            raw = base64.b64decode(inp["raw_base64"])
        else:
            s = inp["json_synth"]
            raw = (s.get("prefix", "") + s.get("repeat", "") * s.get("count", 0)
                   + s.get("suffix", "")).encode("utf-8")
        cls = ijson_class(raw, inp.get("context", "request"))
        verdict = cls is None
        if verdict != expected["valid"]:
            self.fail(f"{rel}: expected valid={expected['valid']}, got "
                      f"{verdict} (error class {cls!r})")
            return
        if not expected["valid"] and "error" in expected and cls != expected["error"]:
            self.fail(f"{rel}: expected error class {expected['error']!r}, "
                      f"got {cls!r}")
            return
        counts["acceptance"] += 1

    def _run_digest_vector(self, rel, inp, expected, counts):
        """Digest vectors re-derive the ratified idempotency-domain
        construction (PROFILE.md §5, D-R0-1): the $domain-tagged JCS
        canonical bytes and the HMAC-SHA-256 under the embedded per-Society
        index key (a test fixture, shape only), emitted as a typed
        scope_erasure_safe DigestRef."""
        try:
            canonical = tagged_jcs(inp["domain"], inp["value"])
        except (TypeError, ValueError) as exc:
            self.fail(f"{rel}: canonicalization failed: {exc}")
            return
        derived_ref = {
            "class": "scope_erasure_safe",
            "algorithm": "hmac-sha-256",
            "key_ref": inp["key_ref"],
            "value_hex": hmac_sha256_hex(inp["index_secret_hex"], canonical),
        }
        ok = True
        if canonical != expected["canonical"]:
            self.fail(f"{rel}: canonical bytes mismatch\n"
                      f"      derived:  {canonical}\n"
                      f"      expected: {expected['canonical']}")
            ok = False
        if derived_ref != expected["digest_ref"]:
            self.fail(f"{rel}: digest_ref mismatch: derived {derived_ref}, "
                      f"expected {expected['digest_ref']}")
            ok = False
        if ok:
            counts["digest"] += 1

    # -- BPA-1 policy vectors (ADR-0001) --

    def _policy_eval(self):
        """Load policy/eval.py (the BPA-1 reference evaluator) once."""
        if not hasattr(self, "_policy_mod"):
            import importlib.util
            path = (Path(__file__).resolve().parent.parent
                    / "policy" / "eval.py")
            mod_spec = importlib.util.spec_from_file_location(
                "bpa1_eval", path)
            mod = importlib.util.module_from_spec(mod_spec)
            mod_spec.loader.exec_module(mod)
            self._policy_mod = mod
            self._policy_cases: list = []  # (rel, input, python result)
        return self._policy_mod

    def _run_policy_vector(self, rel, inp, expected, counts):
        """BPA-1 policy family (ADR-0001, §10.5): re-derive the case through
        the reference evaluator; the result — including typed rejections —
        must equal the golden expected.result byte-for-byte under JCS.
        Cases are retained for the one-shot eval.mjs cross-check."""
        mod = self._policy_eval()
        result = mod.run_case(inp)
        derived = mod.jcs(result)
        golden = mod.jcs(expected.get("result"))
        if derived != golden:
            self.fail(f"{rel}: policy result mismatch\n"
                      f"      derived:  {derived}\n"
                      f"      expected: {golden}")
            return
        self._policy_cases.append((rel, inp, result))
        counts["policy"] += 1

    def cross_check_policy(self) -> str:
        """Replay every policy vector through the independent evaluator
        policy/eval.mjs in one batch; each result must agree (JCS) with the
        Python result (which already matched the golden). Opportunistic like
        the jsonschema backend: without node, run-checks.sh's dedicated
        eval.mjs check and seeded differential still enforce the two-
        evaluator gate."""
        cases = getattr(self, "_policy_cases", [])
        if not cases:
            return "no policy vectors"
        import shutil
        import subprocess
        node = shutil.which("node")
        if node is None:
            return (f"python evaluator only ({len(cases)} vectors; node not "
                    "found — run-checks.sh gates eval.mjs)")
        mod = self._policy_mod
        eval_mjs = (Path(__file__).resolve().parent.parent
                    / "policy" / "eval.mjs")
        proc = subprocess.run(
            [node, str(eval_mjs), "batch"],
            input=json.dumps([inp for _, inp, _ in cases]),
            capture_output=True, text=True)
        if proc.returncode != 0:
            self.fail(f"policy cross-check: eval.mjs batch failed: "
                      f"{proc.stderr.strip()[:400]}")
            return "eval.mjs failed"
        try:
            results = json.loads(proc.stdout)
        except ValueError as exc:
            self.fail(f"policy cross-check: eval.mjs output unparsable: {exc}")
            return "eval.mjs failed"
        if not isinstance(results, list) or len(results) != len(cases):
            self.fail("policy cross-check: eval.mjs returned "
                      f"{len(results) if isinstance(results, list) else '?'} "
                      f"results for {len(cases)} cases")
            return "eval.mjs failed"
        agree = 0
        for (rel, _, py_result), mjs_result in zip(cases, results):
            if mod.jcs(py_result) != mod.jcs(mjs_result):
                self.fail(f"{rel}: evaluators disagree\n"
                          f"      eval.py:  {mod.jcs(py_result)}\n"
                          f"      eval.mjs: {mod.jcs(mjs_result)}")
            else:
                agree += 1
        return (f"both evaluators agree on {agree}/{len(cases)} vectors "
                "(eval.py reference + eval.mjs independent)")

    def _run_tool_call_vector(self, rel, inp, expected, counts):
        """C3a MCP tool-call vectors (spec/vectors/mcp/): the call names a
        profile, a tool, and an input. The shape is valid only when the
        tool exists in exactly that profile's closed tool list in the
        committed mcp/byom-mcp.tools.json AND the input validates against
        the tool's embedded closed input schema AND carries no
        channel-derived field — so a candidate tool under a participant
        envelope and a caller-supplied actor_ref both fail."""
        doc = getattr(self, "_mcp_doc", None)
        if doc is None:
            self.fail(f"{rel}: mcp tools document unavailable (the c3a "
                      "check failed before vectors ran)")
            return
        call = inp["tool_call"]
        env = doc["profiles"].get(call.get("profile"))
        tool = None
        if env is not None:
            tool = next((t for t in env["tools"]
                         if t["name"] == call.get("tool")), None)
        verdict = tool is not None
        if verdict:
            schema = tool["input_schema"]
            value = call.get("input")
            if self.jsonschema is not None:
                verdict = self.jsonschema.Draft202012Validator(
                    schema).is_valid(value)
            else:
                verdict = mini_valid(schema, schema, value)
            if verdict and isinstance(value, dict) \
                    and set(value) & MCP_CHANNEL_DERIVED:
                verdict = False
        if verdict != expected["valid"]:
            self.fail(f"{rel}: expected valid={expected['valid']}, "
                      f"got {verdict}")
            return
        counts["tool-call"] += 1

    def _descriptor_rows(self, machine: str):
        """Load spec/descriptors/<machine>.json once and return its
        transition rows as a set of (from, to, via) triples."""
        cache = getattr(self, "_machine_rows", None)
        if cache is None:
            cache = self._machine_rows = {}
        if machine not in cache:
            path = self.spec_dir / "descriptors" / f"{machine}.json"
            if not path.is_file():
                cache[machine] = None
            else:
                body = strict_parse(path.read_text(encoding="utf-8"))
                cache[machine] = frozenset(
                    (row["from"], row["to"], row["via"])
                    for row in body["transitions"])
        return cache[machine]

    def _run_machine_vector(self, rel, inp, expected, counts):
        """Machine state-walk vectors (spec/vectors/machines/): a transition
        sequence interpreted over the committed descriptor JSON — the §14.8
        closed-machine rule as an executable oracle.

        Interpreter contract:
        - the walk starts at the implicit pre-creation state "absent";
        - {"crash": true} is the crash-marker convention: every
          descriptor-level variable is durable, so a daemon crash/restart
          does not move the state and the walk continues where it stopped
          (§14.8 crash-outcome column);
        - {"from", "via", "to", "expect": "accepted"}: the exact
          (from, to, via) row must exist; the walk advances to `to`;
        - "expect": "rejected": the exact row must NOT exist — an unlisted
          transition is invalid (§14.8) — and the state does not move;
        - "expect": "replay": exact retry of the immediately preceding
          accepted mutation (same via and to). It must be state-idempotent:
          either no row matches from the post-transition state (the guard
          makes the retry a no-op returning the retained receipt) or the
          matching row is a self-transition;
        - expected.final_state pins where the walk must end."""
        machine = inp["machine"]
        rows = self._descriptor_rows(machine)
        if rows is None:
            self.fail(f"{rel}: references unknown descriptor {machine!r}")
            return
        state = "absent"
        last_accepted = None
        ok = True
        for i, step in enumerate(inp.get("steps", ())):
            where = f"{rel}: steps[{i}]"
            if step.get("crash") is True:
                if set(step) != {"crash"}:
                    self.fail(f"{where}: a crash marker carries no other keys")
                    ok = False
                continue  # durable state: the walk resumes unchanged
            missing = {"from", "via", "to", "expect"} - set(step)
            if missing:
                self.fail(f"{where}: missing {sorted(missing)}")
                ok = False
                break
            if step["from"] != state:
                self.fail(f"{where}: from {step['from']!r} but the walk is "
                          f"in {state!r}")
                ok = False
                break
            row = (state, step["to"], step["via"])
            expect = step["expect"]
            if expect == "accepted":
                if row not in rows:
                    self.fail(f"{where}: {row} is not a descriptor row of "
                              f"{machine} (expected accepted)")
                    ok = False
                    break
                state = step["to"]
                last_accepted = (step["via"], step["to"])
            elif expect == "rejected":
                if row in rows:
                    self.fail(f"{where}: {row} IS a descriptor row of "
                              f"{machine} (expected rejected)")
                    ok = False
                    break
            elif expect == "replay":
                if last_accepted != (step["via"], step["to"]):
                    self.fail(f"{where}: replay must retry the immediately "
                              f"preceding accepted mutation {last_accepted}, "
                              f"got ({step['via']!r}, {step['to']!r})")
                    ok = False
                    break
                if row in rows and step["to"] != state:
                    self.fail(f"{where}: replay of {row} would re-execute "
                              f"(descriptor row moves the state) — not "
                              "idempotent")
                    ok = False
                    break
            else:
                self.fail(f"{where}: unknown expect {expect!r}")
                ok = False
                break
        if ok and state != expected.get("final_state"):
            self.fail(f"{rel}: walk ended in {state!r}, expected "
                      f"{expected.get('final_state')!r}")
            ok = False
        if ok:
            counts["machine-walk"] += 1

    # -- entry --

    def run(self) -> int:
        if self.jsonschema is not None:
            from importlib.metadata import version
            backend = f"jsonschema {version('jsonschema')}"
        else:
            backend = "minimal structural validator (jsonschema not installed)"
        n_schemas = self.load_schemas()
        covered = self.check_bundle()
        gw = self.check_governed_work()
        mcp = self.check_mcp_tools()
        desc = self.run_descriptors()
        counts = self.run_vectors()
        policy_note = self.cross_check_policy()
        # taxonomy-bpa1 re-checks vectors already counted as schema
        # vectors, so it stays out of the total.
        total = sum(v for k, v in counts.items() if k != "taxonomy-bpa1")
        print()
        print(f"schemas:  {len(self.schemas)}/{n_schemas} compiled ({backend})")
        print(f"bundle:   {covered}/{len(SLICE_OPS)} B0.1 sheet ops "
              f"schema-covered ({len(SLICE_MUTATING)} mutating, "
              f"{len(SLICE_READS)} reads; complete sheet, "
              f"{len(B01_SHEET)} families)")
        print(f"descriptors: {desc['files']} machines "
              f"({desc['kovee']} kovee-owned), {desc['states']} "
              f"states, {desc['transitions']} transitions — "
              f"{desc['owned']}/{len(SLICE_MUTATING)} mutating ops owned "
              "exactly once")
        print(f"governed-work: {gw['schemas']}/{len(GOVERNED_WORK_SCHEMAS)} "
              f"C2 record schemas, {gw['enums']}/"
              f"{len(GOVERNED_WORK_ENUMS)} design enums verbatim, "
              f"{gw['descriptors']}/{len(GOVERNED_WORK_DESCRIPTORS)} "
              f"kovee-owned descriptors, {gw['taxonomy']}/"
              f"{len(ACT_CLASS_MANDATORY)} Δ4 class arms "
              f"({counts['taxonomy-bpa1']} subjects cross-checked through "
              "policy/eval.py)")
        pending = (f"; pending op schemas: {', '.join(mcp['pending'])}"
                   if mcp["pending"] else "; all ops schema-backed")
        print(f"mcp:      c3a — {mcp['candidate']} candidate + "
              f"{mcp['participant']} participant tools, sheet-exact; "
              f"0 governance/runtime/admin{pending}")
        print(f"vectors:  {total} passed — "
              f"{counts['schema-valid']} schema-valid, "
              f"{counts['schema-invalid']} schema-invalid, "
              f"{counts['acceptance']} acceptance, "
              f"{counts['digest']} digest, "
              f"{counts['machine-walk']} machine-walk, "
              f"{counts['policy']} policy, "
              f"{counts['tool-call']} tool-call")
        print(f"policy:   {policy_note}")
        if self.failures:
            print(f"result:   FAIL ({len(self.failures)} failure(s))")
            return 1
        print("result:   PASS")
        return 0


def main(argv: list[str]) -> int:
    if len(argv) > 1:
        spec_dir = Path(argv[1])
    else:
        spec_dir = Path(__file__).resolve().parent.parent / "spec"
    if not spec_dir.is_dir():
        print(f"FAIL  spec directory not found: {spec_dir}")
        return 1
    return Runner(spec_dir).run()


if __name__ == "__main__":
    sys.exit(main(sys.argv))
