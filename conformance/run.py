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
2. the B0.1 slice bundle op list (registry-derived, §14.6 catalog) is
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
   verdict, raw/synthetic vectors match strict I-JSON + limit acceptance,
   digest vectors re-derive canonical bytes (RFC 8785 JCS, type-tagged as
   JCS([domain, value])) and their SHA-256.

Exit code 0 only when everything passes.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

SAFE_MAX = 2**53 - 1
MAX_REQUEST_BYTES = 262144  # DESIGN.md §14.9: request envelope at most 256 KiB
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

# The B0.1 slice under test here: society + participants + candidates.
SLICE_FAMILIES = ("society", "participants", "candidates")
SLICE_OPS = tuple(op for fam in SLICE_FAMILIES for op in CATALOG[fam])
SLICE_READS = frozenset({"society_show", "participant_show"})
SLICE_MUTATING = tuple(op for op in SLICE_OPS if op not in SLICE_READS)

# Named non-callable kernel/server transitions that may appear as a
# descriptor `via` (§14.8, spec/README.md). `standing_replacement` is the
# gap-note G12 name for the Standing row's operation-less 'replacement'.
NAMED_TRANSITIONS = frozenset({
    "server_time", "activation_admit", "resource_allocate",
    "standing_replacement",
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
    """Strict I-JSON acceptance: duplicate keys, non-finite numbers, and
    unsafe integers fail closed (DESIGN.md §14.2)."""
    def _const(name):
        raise ValueError(f"non-finite number: {name}")

    value = json.loads(
        text, object_pairs_hook=_reject_dup_pairs, parse_constant=_const
    )
    _check_numbers(value)
    return value


def accept_request_bytes(raw: bytes):
    """Pre-schema acceptance of one request envelope's exact bytes."""
    if len(raw) > MAX_REQUEST_BYTES:
        raise ValueError(f"request over 256 KiB: {len(raw)} bytes")
    return strict_parse(raw.decode("utf-8"))


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


def jcs(value) -> str:
    """RFC 8785 JCS restricted to the BPP canonical value domain: objects,
    arrays, strings, safe integers, booleans, null. No BPP canonical value
    contains a float (§14.2, ADR-0001)."""
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
        raise ValueError("floats are not BPP canonical values")
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


def type_tagged_digest(domain: str, value) -> tuple[str, str]:
    canonical = jcs([domain, value])
    return canonical, hashlib.sha256(canonical.encode("utf-8")).hexdigest()


# ------------------------------------------------- schema conventions -------

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
            if node.get("additionalProperties") is not False:
                errs.append(
                    "object schema with properties must set "
                    f"additionalProperties false (near {sorted(node['properties'])[:3]})"
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
    items, minItems, maxItems, uniqueItems."""
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
        slice's op list, not prose, decides schema membership. Every op has a
        closed request/result schema pair; the request pins the exact op
        const; mutations require meta; reads carry none."""
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

    # -- transition descriptors --

    def _descriptor_shape_errors(self, body) -> list[str]:
        errs = []
        if not isinstance(body, dict) or set(body) != {"machine", "states",
                                                       "transitions"}:
            return ["top-level keys must be exactly "
                    "{machine, states, transitions}"]
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
        counts = {"files": 0, "states": 0, "transitions": 0, "owned": 0}
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
            for row in body["transitions"]:
                via = row["via"]
                if via not in ALL_CATALOG_OPS and via not in NAMED_TRANSITIONS:
                    self.fail(f"{name}: via {via!r} is neither a §14.6 "
                              "catalog operation nor a named kernel/server "
                              "transition")
                    continue
                if via in SLICE_READS:
                    self.fail(f"{name}: read operation {via!r} cannot drive "
                              "a transition (reads never mutate, §14.2)")
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
                  "digest": 0}
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
            elif "raw" in inp or "synthetic" in inp:
                self._run_acceptance_vector(rel, inp, expected, counts)
            elif "domain" in inp:
                self._run_digest_vector(rel, inp, expected, counts)
            else:
                self.fail(f"{rel}: unknown vector kind (input keys {sorted(inp)})")
        return counts

    def _run_schema_vector(self, rel, inp, expected, counts):
        schema_name = inp["schema"]
        if schema_name not in self.schemas:
            self.fail(f"{rel}: references unknown schema {schema_name!r}")
            return
        verdict = self._validate(schema_name, inp.get("ref"), inp["value"])
        if verdict != expected["valid"]:
            self.fail(f"{rel}: expected valid={expected['valid']}, got {verdict}")
            return
        counts["schema-valid" if expected["valid"] else "schema-invalid"] += 1

    def _run_acceptance_vector(self, rel, inp, expected, counts):
        if "raw" in inp:
            raw = inp["raw"].encode("utf-8")
        else:
            if inp["synthetic"] != "oversized_request":
                self.fail(f"{rel}: unknown synthetic kind {inp['synthetic']!r}")
                return
            prefix = '{"version":"0.2","op":"hello","pad":"'
            suffix = '"}'
            pad = inp["target_bytes"] - len(prefix) - len(suffix)
            if pad < 0:
                self.fail(f"{rel}: target_bytes too small to synthesize")
                return
            raw = (prefix + "a" * pad + suffix).encode("utf-8")
            if len(raw) != inp["target_bytes"]:
                self.fail(f"{rel}: synthesized {len(raw)} bytes, wanted "
                          f"{inp['target_bytes']}")
                return
        try:
            accept_request_bytes(raw)
            verdict = True
        except ValueError:
            verdict = False
        if verdict != expected["valid"]:
            self.fail(f"{rel}: expected valid={expected['valid']}, got {verdict}")
            return
        counts["acceptance"] += 1

    def _run_digest_vector(self, rel, inp, expected, counts):
        try:
            canonical, digest = type_tagged_digest(inp["domain"], inp["value"])
        except (TypeError, ValueError) as exc:
            self.fail(f"{rel}: canonicalization failed: {exc}")
            return
        ok = True
        if canonical != expected["canonical"]:
            self.fail(f"{rel}: canonical bytes mismatch\n"
                      f"      derived:  {canonical}\n"
                      f"      expected: {expected['canonical']}")
            ok = False
        if digest != expected["sha256_hex"]:
            self.fail(f"{rel}: sha256 mismatch: derived {digest}, "
                      f"expected {expected['sha256_hex']}")
            ok = False
        if ok:
            counts["digest"] += 1

    # -- entry --

    def run(self) -> int:
        if self.jsonschema is not None:
            from importlib.metadata import version
            backend = f"jsonschema {version('jsonschema')}"
        else:
            backend = "minimal structural validator (jsonschema not installed)"
        n_schemas = self.load_schemas()
        covered = self.check_bundle()
        desc = self.run_descriptors()
        counts = self.run_vectors()
        total = sum(counts.values())
        print()
        print(f"schemas:  {len(self.schemas)}/{n_schemas} compiled ({backend})")
        print(f"bundle:   {covered}/{len(SLICE_OPS)} B0.1 slice ops "
              f"schema-covered ({len(SLICE_MUTATING)} mutating, "
              f"{len(SLICE_READS)} reads)")
        print(f"descriptors: {desc['files']} machines, {desc['states']} "
              f"states, {desc['transitions']} transitions — "
              f"{desc['owned']}/{len(SLICE_MUTATING)} mutating ops owned "
              "exactly once")
        print(f"vectors:  {total} passed — "
              f"{counts['schema-valid']} schema-valid, "
              f"{counts['schema-invalid']} schema-invalid, "
              f"{counts['acceptance']} acceptance, "
              f"{counts['digest']} digest")
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
