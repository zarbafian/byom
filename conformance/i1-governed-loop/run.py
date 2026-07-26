#!/usr/bin/env python3
"""I1 governed loop with real intelligence (the integration gate; plan §8 I1).

Both stacks live side by side and the loop crosses BOTH of them: byomd
(this repo) owns every authority record, koveed (../kovee) owns
deliberation, placement and the disclosed metered model broker.

    python3 run.py --scripted            # i1-flow-scripted (gates CI; NO model call)
    python3 run.py --crash-matrix        # i1-crash (both daemons + the broker chain)
    python3 run.py --verify-trails       # i1-trails (per-source attribution)
    python3 run.py --attached-path claude  # i1-attached-claude (gates CI)
    python3 run.py --attached-path codex   # i1-attached-codex  (gates CI)
    python3 run.py --real-model          # i1-real-model (I1_REAL_MODEL=1; REAL providers)
    python3 run.py --harness claude      # i1-flow-claude (I1_REAL_HARNESS=1)
    python3 run.py --harness codex       # i1-flow-codex  (I1_REAL_HARNESS=1)
    python3 run.py --all-checks          # everything deterministic, plus the
                                         # env-gated real harness runs, each
                                         # reported (never silently excluded)

What the scripted gate drives, in order (every step names its owner):

    pinned revisions          the kovee commit the driver was COMPILED
                              against (its build.rs refuses another), every
                              binary rebuilt and checked against cargo's own
                              dependency record
    kovee governance enable   the D10 GREENFIELD saga, live: two inert
                              bindings, then the owner CAS none -> byom, with
                              an OVERLAPPING scope selector attempted and
                              refused
    space + question          kovee's own deliberation records
    attention notice          SENT BY KOVEE's own byom client on byom's
                              narrow attention channel: NOTIFICATION IS NOT
                              A WAKE — no admission, no allocation, no
                              episode from the notice alone
    wake_intent_submit        the PARTICIPANT's own wake, over byom-mcp
    episode_request           byom's kernel stages 2 and 3 (admission +
                              allocation) — BEFORE any placement (L25/A8)
    place / placement_admit   Kovee's PlacementBinding, byom's adapter
    episode_claim/start       the lease, under DUAL fences
    kovee_endeavor_form       the formation saga: exactly one Endeavor, with
                              three refusals attempted first
    call + pledge             the full seat sequence
    act_intent_* + broker     the model_egress act chain to a ONE-SHOT
                              permit, then Kovee's broker: prepared before
                              any dispatch, usage metered back to byom, and
                              BYOM (not kovee) settles; the refusals are
                              driven through koveed's OWN worker-socket
                              `model_complete` as well as the driver
    continuation resume       an Episode yields and a DIFFERENT, HOSTED
                              Manifestation resumes from the portable
                              Continuation through the head CAS
    ambiguous effect          a forced uncertain send walked through
                              effect_outcome_admit (source facts) ->
                              effect_reconcile (governance seat), with the
                              EOA-head-before-disposition-head lock order
                              and the conservative settlement observable
    onboarding compute        the §7.4 one-shot path: the funded intent, the
                              receipt with max_uses 1, and completion as
                              EVIDENCE ONLY

Plan §8's I1 item list is the normative one, and `evidence/<test-id>/
plan-8-i1-coverage.json` names the cell that exercises each item. The gate
FAILS if an item has no cell.

Assurance profile: **developer, confined-unclaimed** — honestly labeled.
The gate claims only that the calls it exercises go through the disclosed,
metered broker; **bypass PREVENTION is not claimed until K4's secure
profile**. All data is synthetic and non-sensitive; the scripted and crash
modes make no network call at all (kovee's own `RecordingTransport` double
stamps `recording-test-double` on every effect it carries).

Who speaks on which channel:
  - governance ops (genesis, offer, admissions, mandate seat + issue, the
    act gate seat + finalize): the direct human channel — byomd's
    governance socket under the operator's uid (`governance:sovereign`);
  - candidate ops: the byom-mcp CANDIDATE profile, over the offer's
    keyless channel binding;
  - agent participant ops (mandate_prepare, activity_open,
    wake_intent_submit, episode_request, pledge_*, act_intent_prepare):
    the byom-mcp PARTICIPANT profile. byomd binds a channel to ONE LIVE
    process, so each session is opened and closed around its steps, and
    the two participant operations byom-mcp does not expose ride a
    short-lived child of this script (`--_agent-call`);
  - human participant ops (call_open, beneficiary seat, review): the
    direct human channel (participant socket, no channel credential);
  - runtime adapters (the attention notice, placement_admit, claim/start,
    yield, usage_report, execution_permit_consume, effect_outcome_admit,
    the onboarding compute permit and Episode): byomd's runtime socket
    under the subject-scoped workload tokens byomd itself published, all
    sent from the kovee side;
  - kovee: the kovee CLI and socket (init, space, question,
    governance_enable, formation saga, invocation, reads), koveed's own
    WORKER socket (`model_complete`, driven at its pre-egress refusals),
    and — for the episode pipeline, the broker and the byom runtime
    channels kovee has no `Workload` arm for — `kovee-driver/`, a binary
    that links kovee's own crates and calls exactly the functions kovee's
    K2 suites call.

Evidence lands in evidence/<test-id>/. Every assertion is made from the
OWNING daemon's own records: byom facts from byomd's events/store, kovee
facts from koveed's events/store — per source, never merged.

Honest residuals of THIS gate. Each one is a LIMIT OF THE STACK, not a
missing cell, and each is also stated in the evidence of the step it
belongs to:

  - **the completing model dispatch still runs in the kovee-linked driver.**
    koveed serves `model_complete` on its worker socket and this gate drives
    that real op — for the refusals, which are decided before any egress —
    but `koveed::Daemon` constructs `HttpsTransport` unconditionally and
    kovee's recording double exists only under kovee-effects' `testing`
    feature, so the daemon has no no-network wire to offer and a COMPLETING
    call through the op would have to reach a real provider. Closing this
    needs one kovee-side change (a `testing`-gated daemon egress); until
    then the completing dispatch goes through `model_broker::complete` in
    the driver, over kovee's sealed `Egress::recording`.
  - **kovee's `kovee-attention` crate is a two-line stub**, so no
    AttentionContract subsystem DECIDES to notify, and kovee has no
    `Workload::Attention` channel class. The notice is sent by kovee's own
    byom client (which verifies the event is in koveed's ledger and derives
    the source digest), reading byomd's attention token; the trigger is
    this scenario's.
  - **kovee ships no onboarding code at all**, so the §7.4 one-shot path is
    driven by kovee's client as the hosted candidate's runtime rather than
    by a kovee subsystem; and byom's
    `onboarding_compute_permit_consume` still demands byom-keyed
    (`local_erasure_safe`) digests for three KOVEE-owned objects, the same
    A8 direction R3-L01 closed for `execution_permit_consume`, so those
    three values come from this scenario.
  - **byom mints ManifestationRevisions only inside `membership_offer`**,
    which fixes `kind: attached_harness`; no byom operation admits a
    `host_kind: kovee_deployment` revision, and `placement_admit` does not
    resolve `selected_manifestation_ref` against that table. The hosted
    Manifestation is therefore the one kovee SELECTS at placement from its
    own active deployment record and byom COMMITS on the Episode and the
    PlacementAdmission — asserted in both stores — not a byom `host_kind`
    row.
  - byom-mcp derives `expected_revision: 1` for `membership_accept`, the
    offer's minted revision, so an onboarding-FUNDED offer (revision 2)
    cannot be accepted through the tool binding; the candidate's own
    channel accepts it directly in that cell.
  - the eligible arm of a notice (an ADOPTED ActivationPolicy) is covered
    by byomd's `b3_attention` suite; this gate asserts the no-effect arm
    and the participant's own four-stage activation.
  - `--real-model` uses kovee's real TLS transport, but the exercised call
    is the only thing claimed: nothing here prevents a bypass of the
    broker. That is K4.
  - the two ATTACHED execution paths are gated deterministically
    (`--attached-path claude|codex`: the real byom-mcp surface, the
    harness's own allowlist and launch argv, the identical tool surface for
    both); the REAL CLI sessions are `--harness claude|codex` under
    I1_REAL_HARNESS=1, which `--all-checks` runs and reports.
  - **a real harness session is held to its MCP WIRE, not to its prose.**
    Every session's byom-mcp stdio is relayed by this file
    (`--_mcp-wire`) and recorded per session, so both harnesses answer to
    the same evidence — the server's own record of the `tools/call` it was
    sent and the answer it returned — and a step passes only when that
    invocation AND byomd's effect event are both there. Neither CLI's
    stdout is evidence of anything: a codex session once answered
    `DONE <identifier>` having made no call at all.
    What the wire cannot say is WHY a model chose what it chose. codex
    (0.145, gpt-5.6-sol) does not enumerate MCP tools in the model's
    visible tool surface at all — they are reachable only through its tool
    search / dynamic-tool object, in every launch configuration tried —
    so a session can be handed a tool and still not look for it. That is a
    harness property, not a byom one: byom-mcp advertised all 34
    participant tools in the same session's `tools/list`. The gate's
    answer is to say so at that step, from the wire, and to run the step
    again (bounded, every attempt recorded) rather than believe the model.

Exit codes: 0 green, 1 failure, 2 honest skip (ungated mode).
"""

import hashlib
import hmac
import json
import os
import re
import shutil
import signal
import socket as socketlib
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
KOVEE_ROOT = Path(os.environ.get("KOVEE_ROOT", str(REPO.parent / "kovee")))
EVIDENCE = HERE / "evidence"
DRIVER_DIR = HERE / "kovee-driver"

AGENT = "part-agent-1"
GOV_ACTOR = "governance:sovereign"
KOVEE_ACTOR = "prin-owner"
REALM = "realm-personal"
FAR_FUTURE = "2030-01-01T00:00:00Z"
PARENT_ACCOUNT = "budget-mandate-i1"
WORST_CASE = 256
BROKER_AUDIENCE = "kovee-model-broker"
ENDPOINT_ROOT = "endpoint-root-i1"
SCOPE = "project:*"

# The scripted and crash modes must not depend on a provider account, and
# must not be able to reach one either. kovee's provider binding is marked
# `disabled` when its key env var is absent, so the deterministic modes
# export this PLACEHOLDER — visibly not a key — and carry every call over
# kovee's `RecordingTransport` double, which never opens a socket.
PLACEHOLDER_KEY = "not-a-key-i1-scripted-recording-transport-only"

# The stub provider reply the recording transport answers with: an exact
# Anthropic Messages response shape, with token counts the assertions
# follow all the way to byom's ledger.
STUB_INPUT_TOKENS = 41
STUB_OUTPUT_TOKENS = 3
STUB_REPLY = json.dumps({
    "id": "msg_01i1scripted",
    "model": "claude-haiku-4-5-20251001",
    "stop_reason": "end_turn",
    "content": [{"type": "text", "text": "OK"}],
    "usage": {"input_tokens": STUB_INPUT_TOKENS,
              "output_tokens": STUB_OUTPUT_TOKENS},
})


# Every daemon this run spawned, in creation order. /tmp is a quota-limited
# tmpfs here, so a mode's `finally` cleans ALL of them — including the ones
# a mid-flow failure never got to return.
LIVE: list = []

# The signal an armed abort raises. Both daemons abort with `std::process::
# abort()` and the broker's `Fault` hooks do the same, so a crash cell may
# require THIS and not merely "a non-zero exit" (R3-I03).
SIGABRT = int(signal.SIGABRT)


def cleanup_live():
    while LIVE:
        LIVE.pop().cleanup()


class Fail(Exception):
    """One failed assertion; the runner reports and exits 1."""


def need(cond, detail):
    if not cond:
        raise Fail(detail)


# ------------------------------------------------------------ binaries ----

_target_cache: dict[str, str] = {}
_built: set = set()


def _target_dir(repo: Path) -> Path:
    key = str(repo)
    if key not in _target_cache:
        meta = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=repo))
        _target_cache[key] = meta["target_directory"]
    return Path(_target_cache[key])


def git_out(repo: Path, *args: str) -> str:
    return subprocess.check_output(["git", "-C", str(repo), *args],
                                   text=True).strip()


def head_of(repo: Path) -> dict:
    return {"commit": git_out(repo, "rev-parse", "HEAD"),
            "dirty": bool(git_out(repo, "status", "--porcelain"))}


def _tracked_blobs(repo: Path) -> dict:
    """path -> blob id, for every file the pinned commit contains."""
    blobs = {}
    for line in git_out(repo, "ls-tree", "-r", "HEAD").splitlines():
        info, path = line.split("\t", 1)
        blobs[path] = info.split()[2]
    return blobs


def source_pin(repo: Path, binaries: list) -> dict:
    """The pin, as EXACT SOURCE STATE rather than HEAD equality (R3-I02).

    `assert_pinned` used to compare commits only, so the confirmer made the
    pinned kovee tree dirty WITHOUT moving HEAD and the gate accepted it —
    it even printed `dirty: true` and passed. A commit id says nothing about
    the bytes that were compiled.

    What is compared here is the set of files cargo itself says each binary
    was built from (`<target>/debug/<name>.d`, the same record
    `assert_fresh` reads). Every one of them that lives in `repo` must be
    TRACKED at HEAD and byte-identical to the blob HEAD holds, and so must
    every manifest and the lockfile. A modified, staged, deleted or
    never-committed source file therefore fails the gate, whatever HEAD
    says.

    It is deliberately not `git status --porcelain`: that would also fail on
    an untracked file in some crate this gate never compiles, which says
    nothing about the revision under test. The dep files name exactly the
    inputs that CAN change what this run observes."""
    blobs = _tracked_blobs(repo)
    inputs: set = set()
    for binary in binaries:
        dep_file = Path(binary).with_suffix(".d")
        need(dep_file.exists(),
             f"cargo wrote no dependency record for {binary}: the source "
             f"state cannot be pinned")
        for line in dep_file.read_text(encoding="utf-8").splitlines():
            if ": " not in line:
                continue
            for token in line.split(": ", 1)[1].split():
                source = Path(token)
                if not source.is_file():
                    continue
                try:
                    rel = str(source.resolve().relative_to(repo.resolve()))
                except ValueError:
                    continue  # a registry/toolchain path, not this repo
                # `build.rs` scripts declare `rerun-if-changed` on git's own
                # files to follow the tree; those are the repository, not
                # source it compiled, and git never tracks them.
                if rel == ".git" or rel.startswith(".git/"):
                    continue
                inputs.add(rel)
    # The manifests and the lockfile decide WHICH sources those are.
    inputs |= {p for p in blobs
               if p == "Cargo.lock" or p.endswith("Cargo.toml")}
    untracked, modified = [], []
    for rel in sorted(inputs):
        if rel not in blobs:
            untracked.append(rel)
            continue
        actual = subprocess.check_output(
            ["git", "-C", str(repo), "hash-object", "--", rel],
            text=True).strip()
        if actual != blobs[rel]:
            modified.append(rel)
    need(not untracked,
         f"{repo.name}: {len(untracked)} compiled input(s) are not committed "
         f"at {git_out(repo, 'rev-parse', 'HEAD')[:12]}, so this run is not "
         f"the pinned revision: {untracked[:8]}")
    need(not modified,
         f"{repo.name}: {len(modified)} compiled input(s) differ from the "
         f"pinned commit {git_out(repo, 'rev-parse', 'HEAD')[:12]} — the "
         f"gate would be testing source that exists nowhere in history: "
         f"{modified[:8]}")
    digest = hashlib.sha256()
    for rel in sorted(inputs):
        digest.update(f"{rel}\0{blobs[rel]}\n".encode())
    return {"compiled_inputs": len(inputs),
            "source_digest": digest.hexdigest(),
            "matches_pinned_commit": True}


def assert_fresh(binary: Path):
    """The staleness oracle, from cargo's OWN dependency record.

    `<target>/debug/<name>.d` lists every source file the artifact was
    compiled from, so this is exact: no guessing which crates a binary
    links, and no false alarm from a test file that cannot change it. A
    binary older than one of its own inputs means the gate would be
    running a revision that no longer exists in the tree (R3-I02)."""
    dep_file = binary.with_suffix(".d")
    need(dep_file.exists(),
         f"cargo wrote no dependency record for {binary.name}: freshness "
         f"cannot be checked")
    stamp = binary.stat().st_mtime
    for line in dep_file.read_text(encoding="utf-8").splitlines():
        if ": " not in line:
            continue
        for token in line.split(": ", 1)[1].split():
            source = Path(token)
            if not source.is_file():
                continue
            need(source.stat().st_mtime <= stamp,
                 f"{binary.name} is older than its own input {source}: the "
                 f"gate would be testing a stale revision")


# One cargo invocation per repo, so each side is resolved with ONE feature
# set: building the packages one at a time made cargo re-resolve the shared
# crates between them and rebuild on every call.
#
# The kovee side is a TEST BUILD (`koveed/testing`), and that is the point:
# it is what lets this gate drive koveed's OWN `model_complete` to
# COMPLETION over a no-network wire (R3-I02), instead of linking kovee as a
# library and choosing the transport itself — which bypassed the very op
# under test. A production `cargo build -p koveed` compiles no
# `RecordingTransport` and no egress override at all; every mode's honesty
# label says which build it ran.
KOVEE_TEST_FEATURE = "koveed/testing"
BUILD_GROUPS = {
    "byom": (["byomd", "byom-cli", "byom-mcp"], []),
    "kovee": (["koveed", "kovee-cli", "kovee-mcp"], [KOVEE_TEST_FEATURE]),
}


def _binary(repo: Path, group: str, name: str) -> str:
    """One daemon/CLI binary, ALWAYS rebuilt once per run (R3-I02:
    "reuse of an existing binary can mix revisions"), then checked against
    its own repo's newest source file — so a build that silently failed to
    pick a change up cannot pass the gate."""
    packages, features = BUILD_GROUPS[group]
    path = _target_dir(repo) / "debug" / name
    if group not in _built:
        args = ["cargo", "build", "-q",
                "--manifest-path", str(repo / "Cargo.toml")]
        for package in packages:
            args += ["-p", package]
        for feature in features:
            args += ["--features", feature]
        subprocess.check_call(args)
        _built.add(group)
    need(path.exists(), f"binary missing after build: {path}")
    assert_fresh(path)
    return str(path)


def byomd_bin():
    return _binary(REPO, "byom", "byomd")


def byom_cli_bin():
    return _binary(REPO, "byom", "byom")


def byom_mcp_bin():
    return _binary(REPO, "byom", "byom-mcp")


def koveed_bin():
    return _binary(KOVEE_ROOT, "kovee", "koveed")


def kovee_cli_bin():
    return _binary(KOVEE_ROOT, "kovee", "kovee")


def kovee_mcp_bin():
    return _binary(KOVEE_ROOT, "kovee", "kovee-mcp")


def driver_bin() -> str:
    """The kovee-side driver: kovee's own crates, linked into one binary
    the scenario can call. Built in its own workspace so it never enters
    byom's lockfile or lints.

    R3-I02, three ways: the driver is ALWAYS rebuilt (a reused binary
    silently mixes kovee revisions); the build is handed the commit this
    run means to gate and the driver's `build.rs` REFUSES TO COMPILE if
    the tree it links is at another one; and the built binary is checked
    against kovee's newest source file, so a stale artifact cannot pass."""
    path = _target_dir(DRIVER_DIR) / "debug" / "i1-kovee-driver"
    kovee = head_of(KOVEE_ROOT)
    subprocess.check_call(["cargo", "build", "-q"], cwd=DRIVER_DIR,
                          env={**os.environ,
                               "I1_KOVEE_COMMIT": kovee["commit"]})
    need(path.exists(), f"driver missing after build: {path}")
    assert_fresh(path)
    return str(path)


def assert_pinned(ev: Evidence) -> dict:
    """The revisions this run gates, asserted rather than assumed — and the
    SOURCE STATE, not just the commit id (R3-I02).

    The driver reports the commit its `build.rs` read out of the kovee tree
    it LINKS (fixed path dependencies, so that tree is decided at compile
    time); this compares it with the tree the harness resolved, and refuses
    a mismatch or a driver built against a path that is not `$KOVEE_ROOT`.
    `$I1_KOVEE_COMMIT`/`$I1_BYOM_COMMIT` pin the pair explicitly when CI
    wants to.

    Then [`source_pin`] takes every file cargo says the binaries under test
    were compiled from and requires it to be committed AT that commit, byte
    for byte. A dirty tree with the right HEAD used to pass — it printed
    `dirty: true` and passed anyway — and it no longer does, from either
    repo, in any mode.

    EVERY mode calls this, including the real-harness ones, which used not
    to call it at all."""
    byom, kovee = head_of(REPO), head_of(KOVEE_ROOT)
    reported = json.loads(subprocess.check_output(
        [driver_bin(), "pinned"], input="{}", text=True))
    need(reported.get("ok"), f"driver `pinned` failed: {reported}")
    built = reported["result"]
    need(built["kovee_commit"] == kovee["commit"],
         f"the driver was built against kovee {built['kovee_commit']}, but "
         f"{KOVEE_ROOT} is at {kovee['commit']}")
    need(Path(built["kovee_path"]).resolve() == KOVEE_ROOT.resolve(),
         f"the driver links {built['kovee_path']}, not $KOVEE_ROOT "
         f"({KOVEE_ROOT})")
    for name, expected, actual in (
            ("I1_KOVEE_COMMIT", os.environ.get("I1_KOVEE_COMMIT"),
             kovee["commit"]),
            ("I1_BYOM_COMMIT", os.environ.get("I1_BYOM_COMMIT"),
             byom["commit"])):
        if expected:
            need(actual.startswith(expected.strip()),
                 f"${name} pins {expected}, the tree is at {actual}")
    byom_sources = source_pin(REPO, [byomd_bin(), byom_cli_bin(),
                                     byom_mcp_bin()])
    kovee_sources = source_pin(KOVEE_ROOT, [koveed_bin(), kovee_cli_bin(),
                                            kovee_mcp_bin(), driver_bin()])
    # The driver's build.rs read the same thing at COMPILE time, from inside
    # cargo — a second, independent witness that the tree it linked was the
    # committed one.
    need(built.get("kovee_worktree_dirty") is False,
         f"the driver was COMPILED against a dirty kovee worktree: the "
         f"binary contains source that is committed nowhere ({built})")
    pinned = {"byom": {**byom, "sources": byom_sources},
              "kovee": {**kovee, "sources": kovee_sources},
              "driver_built_against": built,
              "explicit_pins": {k: os.environ.get(k) for k in
                                ("I1_KOVEE_COMMIT", "I1_BYOM_COMMIT")}}
    ev.blob("pinned-revisions.json", json.dumps(pinned, indent=1))
    return pinned


# ------------------------------------------------------------ evidence ----

class Evidence:
    """Per-test-id evidence: numbered step lines on stdout, a steps.jsonl
    transcript, and named blobs, under evidence/<test-id>/.

    Blobs are NAMESPACED per cell and a duplicate path is a FAILURE
    (R3-I04). The crash matrix used to run seven cells through one
    Evidence with a per-cell driver counter that restarted at 01, so cell
    five's `driver-01-complete.json` silently overwrote cell one's — the
    raw evidence for the earlier kill was simply gone. Now every cell
    calls `namespace()` and a second write to one path raises."""

    def __init__(self, test_id: str):
        self.dir = EVIDENCE / test_id
        shutil.rmtree(self.dir, ignore_errors=True)
        self.dir.mkdir(parents=True)
        self.test_id = test_id
        self.n = 0
        self.ns: str | None = None
        self._written: dict[str, int] = {}
        # (cell, title) for every step this run took. `plan_coverage` reads
        # it: a coverage claim has to point at a step this run actually
        # printed, not at a string its caller composed (R3-I01).
        self.titles: list = []
        self._steps = (self.dir / "steps.jsonl").open("w", encoding="utf-8")

    def namespace(self, slug: str | None):
        """Everything written from here on lands under <slug>/ — one
        directory per cell, so no two cells can collide."""
        self.ns = slug
        if slug is not None:
            (self.dir / slug).mkdir(parents=True, exist_ok=True)

    def step(self, title: str, **detail):
        self.n += 1
        row = {"step": self.n, "title": title, **detail}
        if self.ns:
            row["cell"] = self.ns
        self.titles.append((self.ns, title))
        self._steps.write(json.dumps(row) + "\n")
        self._steps.flush()
        print(f"  ok {self.n:02d}  {title}")

    def path(self, name: str) -> Path:
        return (self.dir / self.ns / name) if self.ns else (self.dir / name)

    def blob(self, name: str, text: str):
        path = self.path(name)
        key = str(path.relative_to(self.dir))
        need(key not in self._written,
             f"duplicate evidence path {key!r} (first written at step "
             f"{self._written.get(key)}): raw evidence must never be "
             f"overwritten")
        self._written[key] = self.n
        path.write_text(text, encoding="utf-8")

    def reserve(self, name: str) -> Path:
        """A path the DRIVER (not this process) will write — reserved so a
        second cell cannot claim it either."""
        path = self.path(name)
        key = str(path.relative_to(self.dir))
        need(key not in self._written, f"duplicate evidence path {key!r}")
        self._written[key] = self.n
        return path

    def close(self):
        self._steps.close()


# ----------------------------------------------- channel proofs (BY-C1) ----

# byomd's candidate/participant credential carries NO key material: the
# file is only the public binding `bpk1.<hex JSON>`. A client CLAIMS its
# channel once over the surface socket (`bpb1.<channel_id>`) and byomd
# answers with a proof key bound to THAT connection's kernel-observed
# peer; every later call carries a FRESH per-call proof
# `bpx1.<channel>.<nonce>.<issued_at>.<mac>`. byomd binds a channel to
# exactly ONE LIVE process, which is why this script never holds an agent
# channel: the MCP servers and the short-lived `--_agent-call` child do.
# (crates/byomd/src/channel.rs; ported here as i0 ported it.)

CHANNEL_PROOF_TAG = "bpp-channel-proof-v0"


def jcs(value) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False).encode()


def tagged_canonical(tag: str, obj: dict) -> bytes:
    return jcs({**obj, "$domain": tag})


def peer_process_start(pid: int) -> int:
    try:
        stat = Path(f"/proc/{pid}/stat").read_text(encoding="utf-8")
    except OSError:
        return 0
    fields = stat.rsplit(")", 1)[-1].split()
    try:
        return int(fields[19])
    except (IndexError, ValueError):
        return 0


# ------------------------------------------------- the wire, losslessly ----
#
# R3-I04(a): the relay used to `json.loads` every frame and throw the bytes
# away, so the oracle's "byte-equal arguments" comparison was really Python
# equality between two parsed dicts — `{"n": 1.0}` and `{"n": 1}` compared
# EQUAL, and a duplicate key silently lost one of its values. What the
# relay saw is now kept, and what is compared is derived from those bytes.
#
# The comparison is BYTE equality of a canonical form that throws nothing
# away: number literals travel verbatim (`1.0` is not `1`), duplicate keys
# are a hard error, and the only things normalised are the two that carry
# no JSON meaning — insignificant whitespace and member ORDER. A model that
# emits the same members in another order sent the same arguments; a model
# that respells a number did not.


class Literal(str):
    """A number exactly as it appeared on the wire."""


def _no_duplicate_keys(items: list) -> dict:
    keys = [k for k, _ in items]
    if len(set(keys)) != len(keys):
        raise ValueError(f"duplicate object keys on the wire: {sorted(keys)}")
    return dict(items)


def wire_value(text: str):
    """One JSON value, parsed WITHOUT losing number spelling or duplicate
    keys — the two distinctions `json.loads` silently discards."""
    return json.loads(text, parse_int=Literal, parse_float=Literal,
                      object_pairs_hook=_no_duplicate_keys)


def canonical(value) -> str:
    """The canonical byte form of a wire value or of a Python value this
    driver fixed. Members are ordered and whitespace is dropped; NOTHING
    else is normalised."""
    if isinstance(value, Literal):
        return str(value)
    if isinstance(value, bool):
        return "true" if value else "false"
    if value is None:
        return "null"
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, list):
        return "[" + ",".join(canonical(v) for v in value) + "]"
    if isinstance(value, dict):
        return "{" + ",".join(
            json.dumps(k, ensure_ascii=False) + ":" + canonical(value[k])
            for k in sorted(value)) + "}"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return repr(value)
    raise Fail(f"not a JSON value: {value!r}")


def json_span(text: str, path: list) -> str | None:
    """The exact substring of `text` holding the value at `path` — the raw
    bytes, quoted verbatim into the evidence."""
    def ws(i: int) -> int:
        while i < len(text) and text[i] in " \t\r\n":
            i += 1
        return i

    def string_end(i: int) -> int:
        i += 1
        while i < len(text):
            if text[i] == "\\":
                i += 2
                continue
            if text[i] == '"':
                return i + 1
            i += 1
        raise ValueError("unterminated string")

    def value_end(i: int) -> int:
        i = ws(i)
        if text[i] == '"':
            return string_end(i)
        if text[i] in "{[":
            close = "}" if text[i] == "{" else "]"
            depth, i = 0, i
            while i < len(text):
                if text[i] == '"':
                    i = string_end(i)
                    continue
                if text[i] in "{[":
                    depth += 1
                elif text[i] in "}]":
                    depth -= 1
                    if depth == 0:
                        if text[i] != close:
                            raise ValueError("mismatched bracket")
                        return i + 1
                i += 1
            raise ValueError("unterminated container")
        while i < len(text) and text[i] not in ",}] \t\r\n":
            i += 1
        return i

    def member(i: int, key: str) -> int | None:
        i = ws(i)
        if text[i] != "{":
            return None
        i = ws(i + 1)
        if text[i] == "}":
            return None
        while True:
            i = ws(i)
            name_end = string_end(i)
            name = json.loads(text[i:name_end])
            i = ws(name_end)
            i = ws(i + 1)  # the ':'
            end = value_end(i)
            if name == key:
                return i
            i = ws(end)
            if i >= len(text) or text[i] != ",":
                return None
            i += 1

    try:
        i = 0
        for key in path:
            found = member(i, key)
            if found is None:
                return None
            i = found
        return text[i:value_end(i)]
    except (ValueError, IndexError):
        return None


# The byom-mcp logical call key, re-derived (D-R1-3, `bridge.rs::meta`).
#
# R3-I04(b): the oracle used to correlate a session with byomd's ledger by
# EVENT KIND, so any same-kind event landing after the mark satisfied the
# step — including one an exact REFUSED invocation could not have produced.
# byom-mcp derives its `request_id` from (session salt, tool, op, JCS of the
# arguments) and byomd stores it as the event's `correlation_ref`, so with
# the salt pinned by this harness the exact event a given call produced is
# nameable. That is request identity, not a kind that anything can share.
MCP_REQUEST_PREFIX = "req-"


def logical_call_key(session: str, tool: str, arguments: dict) -> str:
    need(tool.startswith("byom_"),
         f"byom-mcp tool names derive from their op: {tool}")
    bound = {"session": session, "tool": tool,
             "op": tool[len("byom_"):], "input": arguments}
    return hashlib.sha256(jcs(bound)).hexdigest()[:32]


def correlation_of(session: str, tool: str, arguments: dict) -> str:
    return MCP_REQUEST_PREFIX + logical_call_key(session, tool, arguments)


def parse_credential(line: str) -> dict:
    body = line.strip()
    need(body.startswith("bpk1."),
         f"not a byom channel credential: {body[:12]!r}")
    return json.loads(bytes.fromhex(body[len("bpk1."):]))


def mint_proof(credential_line: str, key: bytes, operation: str) -> str:
    cred = parse_credential(credential_line)
    nonce = os.urandom(16).hex()
    issued_at = int(time.time())
    pid = os.getpid()
    mac = hmac.new(
        key,
        tagged_canonical(CHANNEL_PROOF_TAG, {
            "audience": cred["audience"],
            "channel_id": cred["channel_id"],
            "scope_ref": cred["scope_ref"],
            "operation": operation,
            "binding_ref": cred["binding_ref"],
            "fence_epoch": cred["fence_epoch"],
            "peer_pid": pid,
            "peer_process_start": peer_process_start(pid),
            "nonce": nonce,
            "issued_at": issued_at,
        }),
        hashlib.sha256).hexdigest()
    return f"bpx1.{cred['channel_id']}.{nonce}.{issued_at}.{mac}"


# ------------------------------------------------------------- daemons ----

def _unix_call(path: Path, line: str, preamble: str | None) -> str | None:
    """One request line, one raw reply line. None when the peer died
    before replying (the crash hooks do exactly that)."""
    try:
        s = socketlib.socket(socketlib.AF_UNIX)
        s.settimeout(180)
        s.connect(str(path))
        payload = b""
        if preamble is not None:
            payload += preamble.encode() + b"\n"
        payload += line.encode() + b"\n"
        s.sendall(payload)
        buf = b""
        while not buf.endswith(b"\n"):
            chunk = s.recv(65536)
            if not chunk:
                break
            buf += chunk
        s.close()
    except OSError:
        return None
    if not buf.strip():
        return None
    return buf.decode().rstrip("\n")


class Killable:
    """The crash-oracle half of a daemon handle (R3-I03).

    A cell that kills a daemon must show the kill: the process it armed is
    GONE, it died of the signal the fault raises, and a REPLACEMENT process
    with a different pid answers afterwards. "Some non-zero exit happened"
    is not that — an unrelated startup failure, or a refusal that never
    aborted at all, produced the same green line before."""

    proc: subprocess.Popen | None

    def pid(self) -> int | None:
        return None if self.proc is None else self.proc.pid

    def kill(self):
        if self.proc is not None:
            self.proc.kill()
            self.proc.wait()
            self.proc = None

    def wait_exit(self, timeout: float | None = None) -> int:
        code = 0
        if self.proc is not None:
            code = self.proc.wait(timeout=timeout)
            self.proc = None
        return code

    def died_and_was_replaced(self, old_pid: int | None,
                              expect_signal: int | None,
                              env: dict | None = None) -> dict:
        """Waits for the armed process, requires the expected death, and
        brings a REPLACEMENT up. Returns what happened, for the evidence.

        The wait is BOUNDED: a fault that never fires leaves the daemon
        happily serving, and this cell must then fail rather than hang."""
        need(old_pid is not None, "no armed process to wait for")
        try:
            status = self.wait_exit(timeout=60)
        except subprocess.TimeoutExpired:
            raise Fail(f"pid {old_pid} is still serving after 60s: the "
                       f"armed fault never fired") from None
        need(not _alive(old_pid),
             f"pid {old_pid} is still alive: the armed fault never fired")
        if expect_signal is not None:
            need(status == -expect_signal,
                 f"the armed process exited {status}, not by signal "
                 f"{expect_signal}: a non-zero exit is not the abort the "
                 f"cell claims to have caused")
        self.start(env)          # type: ignore[attr-defined]
        new_pid = self.pid()
        need(new_pid is not None and new_pid != old_pid,
             f"no replacement process came up (old {old_pid}, new {new_pid})")
        return {"armed_pid": old_pid, "exit_status": status,
                "expected_signal": expect_signal, "replacement_pid": new_pid}


def _alive(pid: int) -> bool:
    """Whether a pid we spawned is still running. It is our own child, so
    the kernel keeps it visible until it is reaped, and `wait_exit()` has
    already reaped it — the /proc check is the honest one."""
    return Path(f"/proc/{pid}").exists()


class ByomDaemon(Killable):
    SURFACES = ("governance", "candidate", "participant", "runtime",
                "projection")

    def __init__(self, tag: str, env: dict | None = None):
        self.data_dir = Path(tempfile.mkdtemp(prefix=f"i1-byom-{tag}-data-"))
        self.run_dir = Path(tempfile.mkdtemp(prefix=f"i1-byom-{tag}-run-"))
        self.proc = None
        LIVE.append(self)
        self.start(env)

    def start(self, env: dict | None = None):
        full = {**os.environ, "BYOM_DATA_DIR": str(self.data_dir),
                "BYOM_RUNTIME_DIR": str(self.run_dir)}
        full.pop("BYOMD_ABORT", None)
        full.update(env or {})
        self.proc = subprocess.Popen(
            [byomd_bin()], env=full,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 30
        while True:
            if all((self.run_dir / f"{s}.sock").exists()
                   for s in self.SURFACES):
                reply = _unix_call(self.run_dir / "governance.sock",
                                   json.dumps({"version": "0.2",
                                               "op": "hello"}), None)
                if reply is not None:
                    return
            need(time.time() < deadline, "byomd sockets never came up")
            time.sleep(0.03)

    def call_raw(self, surface: str, line: str,
                 token: str | None = None) -> str | None:
        preamble = token
        if surface == "candidate" and preamble is None:
            preamble = ""
        return _unix_call(self.run_dir / f"{surface}.sock", line, preamble)

    def call(self, surface: str, request: dict,
             token: str | None = None) -> dict:
        raw = self.call_raw(surface, json.dumps(request), token)
        need(raw is not None,
             f"byomd died on {request.get('op')} over {surface}")
        return json.loads(raw)

    def expect_ok(self, surface: str, request: dict,
                  token: str | None = None) -> dict:
        reply = self.call(surface, request, token)
        need(reply.get("outcome") == "ok",
             f"{request.get('op')}: {json.dumps(reply)}")
        return reply

    def incarnation(self) -> str:
        reply = self.expect_ok("governance", {"version": "0.2",
                                              "op": "hello"})
        return reply["result"]["endpoint_incarnation"]

    def channels_dir(self) -> Path:
        return self.data_dir / "channels"

    def token_file(self, name: str) -> Path:
        return self.channels_dir() / name

    def read_token(self, name: str) -> str:
        return self.token_file(name).read_text(encoding="utf-8").strip()

    def row(self, sql: str, key: str) -> str | None:
        """One value out of byomd's OWN database, opened read-only beside
        the running daemon — the inspection channel byom's own fixtures
        use. Only for daemon-DERIVED values the driver must echo back
        (subject digests, seat refs, revisions) and for counting records
        byom owns."""
        conn = sqlite3.connect(f"file:{self.data_dir / 'byom.db'}?mode=ro",
                               uri=True)
        try:
            conn.row_factory = sqlite3.Row
            row = conn.execute(sql, (key,)).fetchone()
        finally:
            conn.close()
        return None if row is None else row[0]

    def count(self, sql: str) -> int:
        conn = sqlite3.connect(f"file:{self.data_dir / 'byom.db'}?mode=ro",
                               uri=True)
        try:
            return int(conn.execute(sql).fetchone()[0])
        finally:
            conn.close()

    def rows(self, sql: str, params: tuple = ()) -> list:
        conn = sqlite3.connect(f"file:{self.data_dir / 'byom.db'}?mode=ro",
                               uri=True)
        try:
            conn.row_factory = sqlite3.Row
            return [dict(r) for r in conn.execute(sql, params).fetchall()]
        finally:
            conn.close()

    def reservations(self, state: str | None = None,
                     account: str = PARENT_ACCOUNT) -> list:
        """byom's own §11.4 reservations on one account — every unit that
        moved is a named holder, so a claim about the ledger can name what
        moved it."""
        sql = ("SELECT holder_kind, holder_ref, amount, state"
               " FROM budget_reservations WHERE account_ref = ?"
               " AND dimension = 'unit'")
        params: tuple = (account,)
        if state is not None:
            sql += " AND state = ?"
            params = (account, state)
        return self.rows(sql, params)

    def ledger(self, account: str = PARENT_ACCOUNT) -> dict:
        conn = sqlite3.connect(f"file:{self.data_dir / 'byom.db'}?mode=ro",
                               uri=True)
        try:
            conn.row_factory = sqlite3.Row
            row = conn.execute(
                "SELECT ceiling, remaining, reserved, committed, uncertain,"
                " delegated_to_children FROM budget_accounts"
                " WHERE account_ref = ? AND dimension = 'unit'",
                (account,)).fetchone()
        finally:
            conn.close()
        need(row is not None, f"no byom budget account {account}")
        led = dict(row)
        led["conserves"] = (
            led["ceiling"] == led["remaining"] + led["reserved"]
            + led["committed"] + led["uncertain"]
            + led["delegated_to_children"])
        return led

    def restart(self, env: dict | None = None):
        self.kill()
        self.start(env)

    def cleanup(self):
        self.kill()
        shutil.rmtree(self.data_dir, ignore_errors=True)
        shutil.rmtree(self.run_dir, ignore_errors=True)


class Koveed(Killable):
    """koveed on its two sockets: the external client surface and the
    disjoint worker surface (§23.3)."""

    def __init__(self, tag: str, byom: ByomDaemon, env: dict | None = None):
        base = Path(tempfile.mkdtemp(prefix=f"i1-kovee-{tag}-"))
        self.base = base
        self.data_dir = base / "data"
        self.run_dir = base / "run"
        self.data_dir.mkdir()
        self.run_dir.mkdir()
        self.byom = byom
        self.proc = None
        LIVE.append(self)
        self.start(env)

    def base_env(self) -> dict:
        return {
            "KOVEE_RUNTIME_DIR": str(self.run_dir),
            # The daemon's OWN configuration: which byomd it is bound to
            # and where that daemon publishes workload tokens. Neither is
            # reachable from a worker request.
            "KOVEE_BYOM_RUNTIME_DIR": str(self.byom.run_dir),
            "KOVEE_BYOM_CHANNELS_DIR": str(self.byom.channels_dir()),
            # R3-I02: the daemon's OWN egress, and the reason this gate can
            # drive koveed's `model_complete` to COMPLETION with no network.
            # It is the daemon that chooses the wire — a worker request
            # cannot name one — so the choice has to be made where the
            # daemon is configured, and only a `koveed/testing` build has
            # this wire to choose. The value is the stub provider reply.
            "KOVEE_TESTING_RECORDING_EGRESS": STUB_REPLY,
        }

    def start(self, env: dict | None = None):
        full = {**os.environ, **self.base_env()}
        full.pop("KOVEED_ABORT", None)
        full.update(env or {})
        self.proc = subprocess.Popen(
            [koveed_bin(), "--data-dir", str(self.data_dir)], env=full,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 30
        while True:
            reply = _unix_call(self.run_dir / "kovee.sock",
                               json.dumps({"version": "0.1", "op": "hello",
                                           "args": {
                                               "supported_versions": ["0.1"],
                                               "implementation": "i1-run",
                                               "implementation_version": "0",
                                               "requested_features": []}}),
                               None)
            if reply is not None:
                return
            need(time.time() < deadline, "koveed socket never came up")
            time.sleep(0.03)

    def store_path(self) -> Path:
        return self.data_dir / "kovee.db"

    def call_raw(self, request: dict, worker: bool = False) -> str | None:
        sock = "kovee-worker.sock" if worker else "kovee.sock"
        return _unix_call(self.run_dir / sock, json.dumps(request), None)

    def call(self, request: dict, worker: bool = False) -> dict:
        raw = self.call_raw(request, worker)
        need(raw is not None, f"koveed died on {request.get('op')}")
        return json.loads(raw)

    def expect_ok(self, request: dict, worker: bool = False) -> dict:
        reply = self.call(request, worker)
        need(reply.get("outcome") == "ok",
             f"{request.get('op')}: {json.dumps(reply)}")
        return reply

    def query(self, sql: str, params: tuple = ()) -> list:
        """koveed's OWN database, read-only beside the running daemon —
        kovee's records, for kovee's claims."""
        conn = sqlite3.connect(f"file:{self.store_path()}?mode=ro", uri=True)
        try:
            conn.row_factory = sqlite3.Row
            return [dict(r) for r in conn.execute(sql, params).fetchall()]
        finally:
            conn.close()

    def count(self, sql: str, params: tuple = ()) -> int:
        return int(self.query(sql, params)[0]["n"])

    def restart(self, env: dict | None = None):
        self.kill()
        self.start(env)

    def cleanup(self):
        self.kill()
        shutil.rmtree(self.base, ignore_errors=True)


# ----------------------------------------------------- request builders ----

def meta(incarnation: str, key: str, expected_revision: int | None = None):
    m = {"request_id": f"req-{key}", "idempotency_key": f"idem-{key}",
         "expected_endpoint_incarnation": incarnation,
         "expected_recovery_epoch": 0}
    if expected_revision is not None:
        m["expected_revision"] = expected_revision
    return m


def digest(seed: int) -> dict:
    """A keyed `local_erasure_safe` digest — byom's class for a local
    object whose verifiability is erased with it."""
    return {"class": "local_erasure_safe", "algorithm": "hmac-sha-256",
            "key_ref": f"i1-test-key-{seed}", "value_hex": f"{seed:02x}" * 32}


def portable(seed: int) -> dict:
    """A cross-boundary `portable_public` digest — both sides recompute."""
    return {"class": "portable_public", "algorithm": "sha-256",
            "value_hex": f"{seed:02x}" * 32}


def keyed_of(text: str) -> dict:
    """A keyed `local_erasure_safe` digest over an exact reference — the
    class byom requires where the object's verifiability is erased with
    the record that cites it."""
    return {"class": "local_erasure_safe", "algorithm": "hmac-sha-256",
            "key_ref": "i1-cause-key",
            "value_hex": hashlib.sha256(text.encode()).hexdigest()}


def kv(op: str, project: str | None, key: str | None, args: dict) -> dict:
    cmd = {"version": "0.1", "op": op, "realm_id": REALM}
    if key is not None:
        cmd["meta"] = {"request_id": f"req-{key}", "idempotency_key": key}
    if project is not None:
        cmd["project_id"] = project
    cmd["args"] = args
    return cmd


# ---------------------------------------------------------- MCP client ----

# Refused tool calls, counted so each one's problem body gets its own
# evidence path (raw evidence is never overwritten, R3-I04).
_MCP_REFUSALS = 0


class Mcp:
    """A scripted MCP stdio client driving a real server binary — the
    harness stand-in: the same JSON-RPC frames Claude Code / Codex send.

    byomd binds a participant channel to ONE LIVE process, so a session is
    opened around its steps and closed before anything else claims it."""

    def __init__(self, argv: list, env: dict, ev: Evidence, tag: str):
        self.proc = subprocess.Popen(
            argv, env={**os.environ, **env},
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True)
        self.next_id = 0
        self.ev = ev
        self.tag = tag
        self.transcript = []
        self.initialize()

    def rpc(self, method: str, params) -> dict:
        self.next_id += 1
        frame = {"jsonrpc": "2.0", "id": self.next_id, "method": method,
                 "params": params}
        self.transcript.append({"dir": "->", **frame})
        self.proc.stdin.write(json.dumps(frame) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        need(line, f"{self.tag}: server closed the stream on {method}")
        reply = json.loads(line)
        self.transcript.append({"dir": "<-", **reply})
        need(reply.get("id") == self.next_id, f"{self.tag}: id mismatch")
        return reply

    def initialize(self):
        reply = self.rpc("initialize", {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "i1-scripted-harness", "version": "0"}})
        need(reply["result"]["protocolVersion"] == "2025-06-18",
             f"{self.tag}: bad protocol version")
        note = {"jsonrpc": "2.0", "method": "notifications/initialized"}
        self.transcript.append({"dir": "->", **note})
        self.proc.stdin.write(json.dumps(note) + "\n")
        self.proc.stdin.flush()

    def tools(self) -> list:
        return self.rpc("tools/list", {})["result"]["tools"]

    def call(self, name: str, arguments: dict):
        reply = self.rpc("tools/call", {"name": name,
                                        "arguments": arguments})
        result = reply["result"]
        return result["content"][0]["text"], bool(result.get("isError"))

    def call_ok(self, name: str, arguments: dict) -> dict:
        text, is_error = self.call(name, arguments)
        if is_error:
            # The problem body is in the server's reply and nowhere else, so
            # a refused call LANDS it — with the frames that led to it —
            # before the failure propagates. A gate that cannot say why a
            # call failed costs the next reader an hour.
            global _MCP_REFUSALS
            _MCP_REFUSALS += 1
            self.ev.blob(
                f"mcp-refusal-{_MCP_REFUSALS:02d}-{name}.json",
                json.dumps({"server": self.tag, "tool": name,
                            "arguments": arguments, "problem_body": text,
                            "frames": self.transcript}, indent=1))
            raise Fail(f"{self.tag}: {name}: {text}")
        return json.loads(text)

    def close(self, frames_name: str | None = None):
        if frames_name:
            self.ev.blob(frames_name,
                         "\n".join(json.dumps(f) for f in self.transcript))
        try:
            self.proc.stdin.close()
        except OSError:
            pass
        self.proc.kill()
        self.proc.wait()


class AgentChannel:
    """The agent's participant channel, opened only for as long as a step
    group needs it. `session()` yields a byom-mcp PARTICIPANT profile."""

    def __init__(self, byom: ByomDaemon, society: str, ev: Evidence):
        self.byom = byom
        self.society = society
        self.ev = ev
        self.sessions = 0

    def salt(self) -> str:
        """The byom-mcp session salt for the CURRENT session.

        byom-mcp derives its logical call key (and therefore the
        `request_id` byomd stores as each event's `correlation_ref`) from
        this salt plus the tool and its JCS arguments. Pinning it is what
        lets a caller name the EXACT event one call produced (R3-I04). It
        must stay per-session: a constant would make two identical calls in
        different sessions share an idempotency key, and the second would
        replay the first's receipt instead of committing."""
        return f"i1-{self.tag}-{self.sessions:03d}"

    tag = "scripted"

    def open(self) -> Mcp:
        self.sessions += 1
        return Mcp([byom_mcp_bin(), "--profile", "participant"],
                   {"BYOM_RUNTIME_DIR": str(self.byom.run_dir),
                    "BYOM_PARTICIPANT_TOKEN_FILE":
                        str(self.byom.token_file(
                            f"participant-{AGENT}.token")),
                    "BYOM_SOCIETY": self.society,
                    "BYOM_MCP_SESSION": self.salt()},
                   self.ev, "byom-mcp[participant]")

    def one(self, tool: str, arguments: dict, frames: str | None = None):
        mcp = self.open()
        try:
            return mcp.call_ok(tool, arguments)
        finally:
            mcp.close(frames)


def channel_socket_call(byom: ByomDaemon, token_file: Path,
                        request: dict) -> dict:
    """One call on a byomd channel surface, made by a SHORT-LIVED child of
    this script (byomd binds a channel to ONE live process). The child
    claims the channel, mints a per-call proof, calls, and exits."""
    proc = subprocess.run(
        [sys.executable, str(HERE / "run.py"), "--_agent-call",
         str(byom.run_dir), str(token_file)],
        input=json.dumps(request), capture_output=True, text=True)
    need(proc.returncode == 0,
         f"channel-call child failed ({proc.returncode}): {proc.stderr}")
    return json.loads(proc.stdout)


def agent_socket_call(byom: ByomDaemon, request: dict) -> dict:
    """One agent-channel call over byomd's participant socket, made by a
    SHORT-LIVED child of this script.

    byom-mcp exposes 36 participant tools but not every participant
    operation (`activation_policy_adopt` has no tool binding yet), and a
    channel is held by one live process — so the call runs in a child that
    claims, calls, and exits, leaving the channel free for the next
    holder. The child is this same file (`--_agent-call`)."""
    return channel_socket_call(
        byom, byom.token_file(f"participant-{AGENT}.token"), request)


def mode_agent_call(run_dir: str, token_file: str) -> int:
    """The `--_agent-call` child: claim the channel, make one call, exit."""
    request = json.load(sys.stdin)
    credential = Path(token_file).read_text(encoding="utf-8").strip()
    cred = parse_credential(credential)
    surface = ("candidate" if cred["audience"] == "candidate"
               else "participant")
    raw = _unix_call(Path(run_dir) / f"{surface}.sock",
                     f"bpb1.{cred['channel_id']}", None)
    if raw is None:
        print("channel claim got no reply", file=sys.stderr)
        return 1
    reply = json.loads(raw)
    if reply.get("outcome") != "ok":
        print(f"channel claim refused: {raw}", file=sys.stderr)
        return 1
    key = bytes.fromhex(reply["result"]["proof_key"])
    proof = mint_proof(credential, key, request["op"])
    answer = _unix_call(Path(run_dir) / f"{surface}.sock",
                        json.dumps(request), proof)
    if answer is None:
        print("no reply", file=sys.stderr)
        return 1
    print(answer)
    return 0


# ------------------------------------------------------- the MCP wire ----

# A harness that swallows a tool error leaves no trace of WHY a governed
# call was refused. codex prints `mcp: byom/<tool> (failed)` and nothing
# else — not the problem type, not the kind, not the detail — and the
# model then narrates a success it never had. Claude Code prints a
# summary of its own. Neither is the server's answer, and the server's
# answer is the only place byomd's problem body exists.
#
# So the harness does not ask the CLI what happened: it puts THIS FILE
# between the harness and the server it names, and records the wire.


def _die_with_parent():
    """`PR_SET_PDEATHSIG`: the relayed server dies with the relay.

    byomd binds a participant channel to ONE LIVE process, so a leaked
    server is not a stray process — it is the channel's holder, and the
    next session's claim is refused while it lives."""
    try:
        import ctypes
        ctypes.CDLL("libc.so.6", use_errno=True).prctl(
            1, int(signal.SIGKILL), 0, 0, 0)          # PR_SET_PDEATHSIG
    except Exception:                                 # pragma: no cover
        pass


def mode_mcp_wire(log_path: str, argv: list) -> int:
    """The `--_mcp-wire` child: run the MCP server `argv` names and relay
    its stdio VERBATIM, recording both directions and its stderr.

    Nothing here interprets, rewrites or retries anything: every line the
    harness sends reaches the server unchanged and every line the server
    answers reaches the harness unchanged. The log is a JSONL transcript
    (`{at, dir, frame|raw}`) — the evidence a failed `tools/call` leaves
    behind, and the record of the tool surface the harness was actually
    served."""
    import threading
    log = open(log_path, "a", buffering=1, encoding="utf-8")

    def record(direction: str, **row):
        log.write(json.dumps({"at": round(time.time(), 3),
                              "dir": direction, **row}) + "\n")

    def frame(direction: str, line: bytes):
        # The RAW text of every frame is kept, always (R3-I04): the parsed
        # form is a convenience, the bytes are the evidence. The oracle
        # compares what the server was sent, not what Python made of it.
        text = line.decode("utf-8", "replace").rstrip("\r\n")
        try:
            record(direction, frame=json.loads(text), raw=text)
        except ValueError:
            record(direction, raw=text)

    server = subprocess.Popen(argv, stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE,
                              preexec_fn=_die_with_parent)
    record("relay", argv=argv, server_pid=server.pid, relay_pid=os.getpid())

    def pump_stdout():
        for line in server.stdout:
            sys.stdout.buffer.write(line)
            sys.stdout.buffer.flush()
            frame("server->harness", line)

    def pump_stderr():
        for line in server.stderr:
            record("server-stderr",
                   text=line.decode("utf-8", "replace").rstrip("\r\n"))

    pumps = [threading.Thread(target=pump_stdout, daemon=True),
             threading.Thread(target=pump_stderr, daemon=True)]
    for pump in pumps:
        pump.start()
    try:
        for line in sys.stdin.buffer:
            frame("harness->server", line)
            server.stdin.write(line)
            server.stdin.flush()
    except OSError:
        pass
    try:
        server.stdin.close()
    except OSError:
        pass
    try:
        code = server.wait(timeout=30)
    except subprocess.TimeoutExpired:
        server.kill()
        code = server.wait()
    for pump in pumps:
        pump.join(timeout=5)
    record("relay", server_exit=code)
    log.close()
    return code if code and code > 0 else 0


def wire_report(log: Path, tool: str) -> dict:
    """What the session's MCP wire says, read back from the relay log.

    - `served`: the tool names the server ADVERTISED in `tools/list` (the
      surface question: was the tool the harness allowed even there?);
    - `invocations`: every `tools/call` for `tool`, each with the server's
      own answer — the problem body VERBATIM when byomd refused — and, for
      each, the RAW argument bytes the relay saw plus their lossless
      canonical form (R3-I04: the comparison used to be Python equality
      between parsed dicts, which cannot tell `1.0` from `1`);
    - `other_calls`: any call for a different tool (the prompt forbids
      them, so this is a finding, not noise);
    - `stderr`: the server's own stderr lines."""
    rows = []
    if log.exists():
        for line in log.read_text(encoding="utf-8").splitlines():
            if line.strip():
                try:
                    rows.append(json.loads(line))
                except ValueError:
                    pass
    served: list = []
    calls: list = []
    pending: dict = {}
    stderr: list = []
    for row in rows:
        if row.get("dir") == "server-stderr":
            stderr.append(row.get("text", ""))
            continue
        body = row.get("frame") or {}
        params = body.get("params") or {}
        if body.get("method") == "tools/call":
            raw_line = row.get("raw")
            raw_arguments = (json_span(raw_line, ["params", "arguments"])
                             if raw_line else None)
            call = {"name": params.get("name"),
                    "arguments": params.get("arguments"),
                    "raw_arguments": raw_arguments,
                    "raw_frame": raw_line}
            if raw_arguments is None:
                # No bytes means no evidence. The step will refuse it
                # rather than fall back to the parsed form.
                call["canonical_arguments"] = None
                call["canonical_error"] = (
                    "the relay recorded no raw bytes for this call's "
                    "arguments")
            else:
                try:
                    call["canonical_arguments"] = canonical(
                        wire_value(raw_arguments))
                except (ValueError, Fail) as bad:
                    call["canonical_arguments"] = None
                    call["canonical_error"] = str(bad)
            pending[json.dumps(body.get("id"))] = call
            continue
        if body.get("method") is not None or "id" not in body:
            continue
        key = json.dumps(body.get("id"))
        result = body.get("result") or {}
        if "tools" in result:
            served = [t.get("name") for t in result["tools"]]
        call = pending.pop(key, None)
        if call is None:
            continue
        content = result.get("content") or []
        answer = content[0].get("text", "") if content else ""
        call["failed"] = bool(result.get("isError")) or "error" in body
        call["answer"] = answer or json.dumps(body.get("error") or result)
        calls.append(call)
    return {"served": served,
            "invocations": [c for c in calls if c["name"] == tool],
            "other_calls": [c for c in calls if c["name"] != tool],
            "stderr": stderr,
            "frames": len(rows)}


# ----------------------------------------------------- the kovee driver ----

class Driver:
    """The kovee-side caller. Every invocation is a short-lived process:
    the crash cells arm a fault that aborts it mid-chain, exactly where
    kovee's own `Fault` hooks abort."""

    def __init__(self, kovee: Koveed, byom: ByomDaemon, ev: Evidence,
                 env: dict | None = None):
        self.kovee = kovee
        self.byom = byom
        self.ev = ev
        self.env = dict(env or {})
        self.calls = 0
        self.last_sends: dict | None = None
        self.last_counter: Path | None = None
        self.last_stdout = ""

    def base(self) -> dict:
        return {"store": str(self.kovee.store_path()),
                "realm": REALM,
                "byom_run_dir": str(self.byom.run_dir),
                "byom_channels_dir": str(self.byom.channels_dir())}

    def run(self, command: str, args: dict,
            expect_ok: bool = True) -> tuple[dict, int]:
        self.calls += 1
        counter = None
        if command == "complete":
            # R3-I03: the send count lands in a file the DRIVER writes and
            # THIS process reads, so a refusal (which returns a problem and
            # no reply body) can still be checked for "not one byte left".
            counter = self.ev.reserve(f"sends-{self.calls:02d}.json")
            args = {**args, "send_counter": str(counter)}
        body = json.dumps({**self.base(), **args})
        proc = subprocess.run(
            [driver_bin(), command], input=body,
            capture_output=True, text=True,
            env={**os.environ, **self.env})
        # The transcript never carries a credential: the driver's inputs
        # are refs and digests, and its environment is not echoed.
        self.ev.blob(f"driver-{self.calls:02d}-{command}.json",
                     json.dumps({"command": command,
                                 "args": json.loads(body),
                                 "exit": proc.returncode,
                                 "stdout": proc.stdout.strip(),
                                 "stderr": proc.stderr.strip()}, indent=1))
        reply = {}
        if proc.stdout.strip():
            try:
                reply = json.loads(proc.stdout.strip().splitlines()[-1])
            except json.JSONDecodeError:
                reply = {}
        self.last_sends = None
        self.last_counter = counter
        if counter is not None and counter.exists():
            self.last_sends = json.loads(counter.read_text(encoding="utf-8"))
        self.last_stdout = proc.stdout.strip()
        if expect_ok:
            need(proc.returncode == 0 and reply.get("ok"),
                 f"driver {command} failed ({proc.returncode}): "
                 f"{proc.stdout.strip()} {proc.stderr.strip()}")
        return reply, proc.returncode

    def ok(self, command: str, args: dict) -> dict:
        reply, _ = self.run(command, args)
        return reply["result"]

    def problem(self, command: str, args: dict) -> dict:
        reply, code = self.run(command, args, expect_ok=False)
        need(code != 0 and not reply.get("ok"),
             f"driver {command} was expected to refuse: {reply}")
        return reply.get("problem") or {"error": reply.get("error")}

    def durable_sends(self) -> int:
        """The externally recorded send count of the last `complete` — the
        number the error paths used to drop. Absent means the process never
        got to report it (an armed abort), which is a different claim and
        never reads as zero."""
        need(self.last_sends is not None,
             "the driver wrote no durable send counter: 'not one byte left' "
             "cannot be claimed from an absent file")
        return int(self.last_sends["sends"])

    def no_send_counter(self) -> bool:
        """True when the driver died before it could report — the only
        honest reading of an absent counter."""
        return self.last_sends is None


def kovee_problem_kind(problem: dict) -> str:
    return str(problem.get("type") or problem.get("error") or "")


# ------------------------------------------------------------- helpers ----

def cli(argv: list, env: dict, ev: Evidence, name: str,
        expect_ok: bool = True) -> subprocess.CompletedProcess:
    proc = subprocess.run(argv, env={**os.environ, **env},
                          capture_output=True, text=True)
    ev.blob(name, f"$ {' '.join(argv)}\n--- exit {proc.returncode}\n"
                  f"--- stdout\n{proc.stdout}--- stderr\n{proc.stderr}")
    if expect_ok:
        need(proc.returncode == 0,
             f"{' '.join(argv)} failed ({proc.returncode}): {proc.stderr}")
    return proc


def timeline(byom: ByomDaemon, cursor: str) -> list:
    reply = byom.expect_ok("projection", {
        "version": "0.2", "op": "events_read",
        "continuation": cursor, "page_size": 512})
    return reply["result"]["events"]


def assert_ordered(kinds: list, expected: list, source: str):
    pos = 0
    for want in expected:
        found = [i for i in range(pos, len(kinds)) if kinds[i] == want]
        need(found, f"{source} timeline missing {want} after index {pos}: "
                    f"{kinds}")
        pos = found[0] + 1


def sovereign_id(byom: ByomDaemon, society: str) -> str:
    snap = byom.expect_ok("projection", {
        "version": "0.2", "op": "snapshot_get", "society_id": society,
        "kinds": ["participants"]})
    for p in snap["result"]["participants"]:
        if p.get("kind") == "human":
            return p["participant_id"]
    raise Fail("no sovereign human participant in the snapshot")


def kovee_events(kovee: Koveed, project: str) -> list:
    reply = kovee.expect_ok(kv("events_read", project, None,
                               {"source": project, "limit": 512}))
    return reply["result"]["events"]


def activation_rows(byom: ByomDaemon) -> dict:
    """The four records only the four-stage activation may create."""
    return {
        "wake_intents": byom.count("SELECT COUNT(*) FROM wake_intents"),
        "activation_admissions":
            byom.count("SELECT COUNT(*) FROM activation_admissions"),
        "resource_allocations":
            byom.count("SELECT COUNT(*) FROM resource_allocations"),
        "episodes": byom.count("SELECT COUNT(*) FROM episodes"),
    }


# ------------------------------------------------------- byom side setup ----

# The whole I1 arc, in the order byomd's OWN ledger must show it. The
# four kernel stages sit between the participant's wake and the placement,
# and the act chain sits between the Pledge and the settlement.
BYOM_EXPECTED_ORDER = [
    "society.prepared", "society.genesis", "charter.adopted",
    "budget.roots_established",
    "membership.offered", "manifestation.proposed",
    "channel.candidate_minted", "membership.accepted",
    "membership.admitted", "channel.converted", "manifestation.admitted",
    "mandate.prepared", "mandate.position_recorded", "mandate.issued",
    "budget.reserved",
    "activity.opened",
    "attention-notice.recorded",          # notification...
    "wake-intent.submitted",              # ...is not a wake
    "activation-admission.admitted",      # stage 2 (kernel)
    "resource-allocation.reserved",       # stage 3 (kernel)
    "episode.eligible",                   # requested, NOT queued
    "subordinate-reservation.confirmed",  # stage 4 (Kovee adapter)
    "episode.queued", "episode-lease.claimed", "episode.running",
    "endeavor.position_recorded", "endeavor.finalized",
    "kovee.endeavor_formed",
    "call.opened",
    "pledge.proposed", "pledge.position_recorded", "pledge.committed",
    "act-intent.prepared", "act-intent.awaiting_decision",
    "act-intent.authorized", "act-intent.consumed",
    "subordinate-reservation.settled",
    "delivery.submitted", "review.recorded",
    "subordinate-reservation.released", "episode.completed",
]

# Which actor authors each byom event kind in this loop. EXHAUSTIVE: the
# trail check FAILS on any kind it does not list, so no record can pass
# unchecked — and the interesting rows are the ones no caller may author.
def byom_actor_rules(sov: str) -> dict:
    gov, agent = GOV_ACTOR, f"participant:{AGENT}"
    human = f"participant:{sov}"
    kernel = "kernel:activation"
    principal = "kovee-principal:prin-owner"
    exact = {
        # governance, on the direct human channel
        "society.prepared": gov, "society.genesis": gov,
        "charter.adopted": gov, "participant.admitted": gov,
        "budget.roots_established": gov, "membership.offered": gov,
        "participant.proposed": gov, "manifestation.proposed": gov,
        "channel.candidate_minted": gov, "membership.admitted": gov,
        "channel.converted": gov, "manifestation.admitted": gov,
        "mandate.position_recorded": gov, "mandate.issued": gov,
        "act-intent.awaiting_decision": gov, "act-intent.authorized": gov,
        # the agent, over its own participant channel
        "mandate.prepared": agent, "activity.opened": agent,
        "wake-intent.submitted": agent, "episode.eligible": agent,
        "pledge.proposed": agent, "pledge.committed": agent,
        "act-intent.prepared": agent, "delivery.submitted": agent,
        # the human participant seat
        "call.opened": human, "review.recorded": human,
        # the byom KERNEL: no caller authors an admission or an allocation
        "activation-admission.admitted": kernel,
        "resource-allocation.reserved": kernel,
        "episode.queued": kernel,
        # kovee's NARROW adapters, each on its own subject-scoped channel
        "attention-notice.recorded": "kovee-adapter:attention",
        "subordinate-reservation.confirmed": "kovee-adapter:placement",
        "subordinate-reservation.settled": "kovee-adapter:meter",
        "subordinate-reservation.released": "kovee-adapter:meter",
        "act-intent.consumed": "kovee-adapter:effect-service",
        # kovee's DELEGATED PRINCIPAL channel (the formation saga)
        "endeavor.position_recorded": principal,
        "endeavor.finalized": principal,
        "kovee.endeavor_formed": principal,
        "budget.delegated": principal,
        # the CONTINUATION is the participant's own private state (§11.3)
        "continuation.written": agent,
        # the §13.2 effect axes: the SOURCE fact rides kovee's narrow
        # effect-outcome adapter, the LOCAL consequence is a governance
        # seat — two records, two owners, never one
        "effect-outcome-admission.admitted": "kovee-adapter:effect-outcome",
        "effect-governance-disposition.recorded": gov,
        # the §7.4 onboarding path: the Society funds, kovee's broker
        # consumes the one-shot compute, the hosted runtime claims
        "onboarding-activation-offer.offered": gov,
        "onboarding-compute-intent.authorized": gov,
        "onboarding-compute-intent.consumed": "kovee-adapter:model-broker",
        "onboarding-episode.completed": "kovee-adapter:model-broker",
    }
    prefix = {
        # the workload identity that holds the lease, and the candidate
        # channel that closes at admission
        "episode-lease.claimed": "runtime:",
        "episode.running": "runtime:",
        "episode.yielded": "runtime:",
        "episode.completed": "runtime:",
        "pledge.underway": "participant:",
        "membership.accepted": "candidate:",
        # the hosted candidate's own workload, named by its runtime binding
        "onboarding-episode.claimed": "candidate-runtime:",
    }
    either = {
        # both the governance seat (mandate) and the agent (pledge, act)
        # reserve budget on their own account
        "budget.reserved": (gov, agent),
        "standing.activated": (gov,),
        "pledge.position_recorded": (agent, human),
        # the review's own budget settlement rides the reviewer's channel
        "budget.settled": (human, agent, gov),
        "budget.released": (human, agent, gov, "kovee-adapter:meter"),
    }
    return {"exact": exact, "prefix": prefix, "either": either}


def verify_byom_attribution(events: list, sov: str) -> list:
    rules = byom_actor_rules(sov)
    table = []
    unmapped = sorted({e["kind"] for e in events
                       if e["kind"] not in rules["exact"]
                       and e["kind"] not in rules["prefix"]
                       and e["kind"] not in rules["either"]})
    need(not unmapped,
         f"unchecked byom event kinds {unmapped}: the attribution map must "
         f"be exhaustive")
    for e in events:
        kind, actor = e["kind"], e["actor_ref"]
        need(actor, f"byom event without an actor: {e}")
        if kind in rules["exact"]:
            expected = rules["exact"][kind]
            need(actor == expected,
                 f"{kind} authored by {actor!r}, expected {expected!r}")
            label = expected
        elif kind in rules["prefix"]:
            expected = rules["prefix"][kind]
            need(actor.startswith(expected),
                 f"{kind} authored by {actor!r}, expected {expected}*")
            label = f"{expected}*"
        elif kind in rules["either"]:
            allowed = rules["either"][kind]
            need(actor in allowed,
                 f"{kind} authored by {actor!r}, expected one of {allowed}")
            label = " | ".join(allowed)
        else:
            raise Fail(f"unchecked byom event kind {kind!r} (actor "
                       f"{actor!r}): the attribution map must be "
                       f"exhaustive")
        table.append({"kind": kind, "actor_ref": actor, "expected": label})
    return table


def bootstrap_society(byom: ByomDaemon, tag: str, ev: Evidence) -> dict:
    """Genesis on the direct human channel: the human is the genesis
    actor and Kovee never is."""
    inc = byom.incarnation()
    prepared = byom.expect_ok("governance", {
        "version": "0.2", "op": "society_prepare",
        "meta": meta(inc, f"{tag}-prep"),
        "home_authority_ref": "auth-home-1",
        "proposed_charter_ref": "charter-draft-1",
        "proposed_charter_digest": digest(0xA1),
        "classification_binding_ref": "class-bind-1",
        "classification_binding_digest": digest(0xA2)})
    society = prepared["result"]["society_id"]
    booted = byom.expect_ok("governance", {
        "version": "0.2", "op": "society_bootstrap",
        "meta": meta(inc, f"{tag}-boot", 1),
        "society_id": society,
        "preparation_ref": prepared["result"]["preparation_ref"],
        "subject_digest": prepared["result"]["subject_digest"]})
    return {"society": society, "genesis": booted["source_cursor"],
            "incarnation": inc}


def onboard_agent(byom: ByomDaemon, society: str, genesis: str, tag: str,
                  ev: Evidence) -> dict:
    """The offer, the candidate's own acceptance over byom-mcp, and the
    TWO governance admissions."""
    inc = byom.incarnation()
    subject = digest(0xB1)
    offered = byom.expect_ok("governance", {
        "version": "0.2", "op": "membership_offer",
        "meta": meta(inc, f"{tag}-offer"),
        "participant_ref": AGENT,
        "proposed_standing_ref": "standing-proposal-1",
        "subject_digest": subject,
        "offered_by_decision_ref": f"dec-society-{society}",
        "expires_at": FAR_FUTURE})
    offer_id = offered["result"]["offer_id"]
    manifestation = None
    for e in timeline(byom, genesis):
        if e["kind"] == "manifestation.proposed":
            manifestation = e["object_ref"]
    need(manifestation is not None, "no manifestation.proposed event")

    cand = Mcp([byom_mcp_bin(), "--profile", "candidate"],
               {"BYOM_RUNTIME_DIR": str(byom.run_dir),
                "BYOM_CANDIDATE_TOKEN_FILE":
                    str(byom.token_file(f"candidate-{offer_id}.token"))},
               ev, "byom-mcp[candidate]")
    try:
        accepted = cand.call_ok("byom_membership_accept",
                                {"offer_ref": offer_id,
                                 "subject_digest": subject})
        need(accepted["result"]["offer_state"] == "accepted",
             f"accept: {accepted}")
        acceptance = accepted["result"]["acceptance_id"]
    finally:
        cand.close("byom-mcp-candidate-frames.jsonl")

    byom.expect_ok("governance", {
        "version": "0.2", "op": "participant_admit",
        "meta": meta(inc, f"{tag}-admit", 2),
        "offer_ref": offer_id,
        "membership_acceptance_ref": acceptance,
        "admitted_by_decision_ref": f"dec-offer-{offer_id}",
        "admission_subject_digest": subject})
    byom.expect_ok("governance", {
        "version": "0.2", "op": "manifestation_admit",
        "meta": meta(inc, f"{tag}-manif", 1),
        "manifestation_ref": manifestation,
        "admitted_by_decision_ref": f"dec-manif-{manifestation}"})
    need(byom.token_file(f"participant-{AGENT}.token").exists(),
         "the participant channel credential must be minted at admission")
    epoch = int(byom.row("SELECT binding_epoch FROM participants"
                         " WHERE participant_id = ?", AGENT) or 1)
    return {"offer": offer_id, "acceptance": acceptance,
            "manifestation": manifestation, "binding_epoch": epoch,
            "subject": subject}


def issue_mandate(byom: ByomDaemon, agent: AgentChannel, tag: str) -> dict:
    """The bounded Mandate: prepared by the grantee over its own channel,
    positioned by the human seat, issued by governance with its budget
    reservation. `model_egress` is one of the Δ4 act classes it bounds —
    without it no model call can even be prepared."""
    inc = byom.incarnation()
    mprep = agent.one("byom_mandate_prepare", {
        "grantee_participant_ref": AGENT,
        "purpose_ref": "purpose-explore-i1",
        "allowed_operations": ["activity_open", "continuation_write",
                               "wake_intent_submit", "model_egress"],
        "resource_selectors": ["res-repo-i1"],
        "data_class_selectors": ["class-public"],
        "destination_selectors": [],
        "budget_ceiling_set_ref": PARENT_ACCOUNT,
        "concurrency_ceiling": 8,
        "delegation": {"allowed": False, "max_depth": 0, "max_children": 0,
                       "grantee_selectors": []},
        "expires_at": FAR_FUTURE})
    mandate = mprep["result"]["mandate_id"]
    seat = mprep["result"]["required_seat_refs"][0]
    subject = mprep["result"]["subject_digest"]
    byom.expect_ok("governance", {
        "version": "0.2", "op": "mandate_position",
        "meta": meta(inc, f"{tag}-mpos"),
        "proposal_ref": mandate, "proposal_revision": 1,
        "subject_digest": subject, "seat_ref": seat, "value": "assent"})
    issued = byom.expect_ok("governance", {
        "version": "0.2", "op": "mandate_issue",
        "meta": meta(inc, f"{tag}-missue", 1),
        "mandate_id": mandate, "subject_digest": subject})
    return {"mandate": mandate, "seat": seat, "subject_digest": subject,
            "revision": issued["result"]["revision"]}


# ---------------------------------------------------- the scripted flow ----

def kovee_bind_governance(kovee: Koveed, byom: ByomDaemon, society: str,
                          driver: Driver, ev: Evidence) -> dict:
    """`kovee governance enable` — the D10 GREENFIELD binding saga, live:
    two INERT bindings, then the owner CAS `none -> byom`, under a frozen
    authority-registry row. Kovee is never the genesis actor: the Society
    must already exist and be `active`, which the saga verifies through
    byomd's own projection surface."""
    before = kovee.expect_ok(kv("governance_show", None, None, {}))
    need(before["result"]["governance_owner"] == "none",
         f"a fresh realm has no governance owner: {before}")
    enabled = kovee.expect_ok(kv("governance_enable", None, "idem-i1-enable",
                                 {"byom_endpoint_ref": "local",
                                  "society_ref": society,
                                  "exact_scope_selector": SCOPE,
                                  "allowed_project_and_space_selectors":
                                      [SCOPE],
                                  "classification_binding_ref":
                                      "class-bind-i1",
                                  "expected_owner_revision": 0}))
    result = enabled["result"]
    need(result["state"] == "active", f"greenfield state: {result['state']}")
    need(result["binding"]["status"] == "active"
         and result["mapping"]["status"] == "active",
         f"bindings must be active after the CAS: {result}")
    owner = result["owner_binding"]
    need(owner["governance_owner"] == "byom" and owner["revision"] == 2
         and owner["owner_endpoint_ref"] == "local",
         f"owner CAS none -> byom: {owner}")
    need(result["binding"]["endpoint_incarnation"] == byom.incarnation(),
         "the binding pins the endpoint incarnation byomd reports")
    need(result["mapping"]["society_ref"] == society,
         "the mapping names the Society byomd bootstrapped")
    # Crash-safe by construction: the exact retry returns the same
    # binding rather than a second one.
    again = kovee.expect_ok(kv("governance_enable", None, "idem-i1-enable",
                               {"byom_endpoint_ref": "local",
                                "society_ref": society,
                                "exact_scope_selector": SCOPE,
                                "allowed_project_and_space_selectors":
                                    [SCOPE],
                                "classification_binding_ref":
                                    "class-bind-i1",
                                "expected_owner_revision": 0}))
    need(again["result"]["binding"]["binding_ref"]
         == result["binding"]["binding_ref"],
         "an exact retry must return the identical binding")
    need(kovee.count("SELECT COUNT(*) AS n FROM kovee_realm_byom_bindings")
         == 1, "a retry must not create a second binding")
    # R3-I04: "overlap impossible" was NARRATED and never attempted. Both
    # of these are now tried against the live daemon and must be refused.
    overlapping = kovee.call(kv("governance_enable", None, "idem-i1-overlap",
                                {"byom_endpoint_ref": "local",
                                 "society_ref": society,
                                 # `project:*` is already owned, and this
                                 # extends it, so the two selectors overlap.
                                 "exact_scope_selector":
                                     "project:proj-i1-overlap/space:sp-1",
                                 "allowed_project_and_space_selectors":
                                     ["project:proj-i1-overlap/space:sp-1"],
                                 "classification_binding_ref":
                                     "class-bind-i1",
                                 "expected_owner_revision":
                                     owner["revision"]}))
    need(overlapping.get("outcome") != "ok",
         f"an OVERLAPPING governed scope must be refused, not enabled: "
         f"{overlapping}")
    overlap_detail = json.dumps(overlapping.get("problem") or {})
    need("overlap" in overlap_detail,
         f"the refusal must name the overlap rule: {overlapping}")
    # A re-enable carrying CHANGED members cannot re-configure the seam.
    # kovee answers "already active" for an owned scope (the D10 saga has
    # nothing to do), and this checks that the answer is the COMMITTED
    # configuration, not the one the caller just asked for.
    changed = kovee.expect_ok(kv("governance_enable", None,
                                 "idem-i1-reconfigure",
                                 {"byom_endpoint_ref": "local",
                                  "society_ref": society,
                                  "exact_scope_selector": SCOPE,
                                  "allowed_project_and_space_selectors":
                                      [SCOPE],
                                  "classification_binding_ref":
                                      "class-bind-OTHER",
                                  "expected_owner_revision": 0}))
    need(changed["result"]["mapping"]["classification_binding_ref"]
         == "class-bind-i1",
         f"a re-enable must not re-configure the committed mapping: "
         f"{changed['result']['mapping']}")
    need(changed["result"]["binding"]["binding_ref"]
         == result["binding"]["binding_ref"],
         "and it names the same binding")
    need(kovee.count("SELECT COUNT(*) AS n FROM kovee_realm_byom_bindings")
         == 1,
         "no refusal or re-enable may leave a second binding behind")
    need(kovee.expect_ok(kv("governance_show", None, None, {}))["result"]
         ["governance_owner"] == "byom",
         "and the owner binding is untouched throughout")
    result["refused_overlap"] = overlapping.get("problem")
    result["reconfigure_ignored"] = {
        "sent_classification_binding_ref": "class-bind-OTHER",
        "committed": changed["result"]["mapping"][
            "classification_binding_ref"]}
    return result


def install_host_binding(driver: Driver, byom: ByomDaemon, enabled: dict,
                         ev: Evidence) -> dict:
    """Amendment A2's "Kovee may start, configure and bind byomd, and
    supply inert context only": the wire projection of the very binding
    Kovee committed, derived by KOVEE's own `hostint` so byomd recomputes
    the same cross-boundary digests. byomd is restarted to read it; the
    endpoint incarnation is persistent, so this is not a re-incarnation."""
    incarnation = byom.incarnation()
    document = driver.ok("host-binding", {
        "binding": enabled["binding"], "mapping": enabled["mapping"],
        "endpoint_root_id": ENDPOINT_ROOT,
        "byom_data_dir": str(byom.data_dir)})
    byom.restart()
    need(byom.incarnation() == incarnation,
         "a restart must not re-incarnate the endpoint")
    binding_ref = document["binding_ref"]
    recovery = byom.token_file(f"recovery-workload-{binding_ref}.token")
    need(recovery.exists(),
         f"byomd published no recovery-workload token for {binding_ref}")
    ev.blob("byom-host-binding.json", json.dumps(document["document"],
                                                 indent=1))
    return {"binding_ref": binding_ref,
            "recovery_token": str(recovery),
            "issuers": document["delegated_principal_issuers"]}


def kovee_deliberation(kovee: Koveed, ev: Evidence) -> dict:
    """kovee's own side: a project, a space, and the human's question."""
    env_cli = {"KOVEE_RUNTIME_DIR": str(kovee.run_dir)}
    init = cli([kovee_cli_bin(), "init"], env_cli, ev, "kovee-cli-init.txt")
    match = re.search(r"project:\s+(\S+)", init.stdout)
    need(match, f"kovee init printed no project: {init.stdout}")
    project = match.group(1)
    created = cli([kovee_cli_bin(), "space", "create", "--project", project,
                   "--title", "Flaky checkout tests"],
                  env_cli, ev, "kovee-cli-space-create.txt")
    space_result = json.loads(created.stdout)
    space = space_result["space_id"]
    branch = space_result["main_branch_id"]
    question = cli([kovee_cli_bin(), "space", "contribute",
                    "--project", project, "--space", space,
                    "--kind", "question", "--text",
                    "Why does checkout flake under load?"],
                   env_cli, ev, "kovee-cli-question.txt")
    q = json.loads(question.stdout)
    need(q["kind"] == "question", f"question kind: {q}")
    # The committed event the AttentionContract would notify byom about —
    # kovee's own event id, read from kovee's own ledger.
    event = None
    for e in kovee_events(kovee, project):
        if e["type"] == "dev.kovee.space.contribution-appended.v1":
            event = e
    need(event is not None, "no contribution event in kovee's ledger")
    return {"project": project, "space": space, "branch": branch,
            "question": q["contribution_id"], "event": event}


def attention_notice(driver: Driver, stream: str, event: dict, key: str,
                     generation: int = 1) -> dict:
    """kovee Attention may NOTIFY byom's adapter of an admitted exact
    event; byom alone decides whether a Participant's WakeIntent and
    ActivityStream permit a new Episode (byom §16.4, family contract L25).

    R3-I01 (f): the notice is now sent by KOVEE — the kovee-linked driver
    verifies the event is in koveed's OWN ledger, derives the
    cross-boundary `source_event_digest` with kovee's own hashing, and
    sends over kovee's own byom client (`Endpoint::call_with_preamble`) on
    byomd's narrow attention channel, under the token byomd published for
    this exact ActivityStream generation.

    What is STILL not kovee's, said plainly: kovee's `kovee-attention`
    crate is a two-line stub, so no AttentionContract subsystem exists to
    DECIDE that this event deserves a notice, and kovee has no
    `Workload::Attention` channel class — the driver reads byomd's token
    file for it. The trigger is the scenario's; the record, the surface,
    the digest and every refusal are the daemons' own."""
    return driver.ok("attention-notice", {
        "activity_stream_ref": stream,
        "generation": generation,
        "source_event_ref": event["event_id"],
        "stable_notice_key": f"notice-{key}"})


def governed_setup(ev: Evidence, tag: str, provider_env: dict,
                   agent_factory=None, stop_after: str | None = None) -> dict:
    """Everything up to and including the committed Pledge: both daemons
    live, the greenfield binding, the deliberation records, onboarding, the
    Mandate, the attention notice that is not a wake, the four-stage
    activation, the formation saga and the Pledge.

    `provider_env` is the credential environment the DAEMON and the driver
    are started in — it is never written to evidence. `agent_factory`
    builds the agent-side caller, so a real harness session can stand
    where the scripted MCP client stands."""
    byom = ByomDaemon(tag, provider_env)
    kovee = Koveed(tag, byom, provider_env)
    driver = Driver(kovee, byom, ev, provider_env)
    ctx = {"byom": byom, "kovee": kovee, "driver": driver}
    inc = byom.incarnation()

    # 1. Genesis on the direct human channel.
    booted = bootstrap_society(byom, tag, ev)
    society, genesis = booted["society"], booted["genesis"]
    sov = sovereign_id(byom, society)
    ctx.update(society=society, genesis=genesis, sovereign=sov)
    ev.step("byom: society_prepare + society_bootstrap (governance, direct "
            "human channel) — atomic genesis; the human is the genesis "
            "actor and Kovee never is",
            society_id=society, sovereign=sov, genesis_cursor=genesis)

    # 2. kovee's deliberation records.
    delib = kovee_deliberation(kovee, ev)
    ctx.update(delib)
    ev.step("kovee: `kovee init` + space create + the human's question "
            "(CLI) — kovee's own records, in kovee's own ledger",
            project_id=delib["project"], space_id=delib["space"],
            question=delib["question"],
            kovee_event=delib["event"]["event_id"])

    # 3. The greenfield binding saga, live.
    enabled = kovee_bind_governance(kovee, byom, society, driver, ev)
    ev.step("kovee: governance_enable — the D10 GREENFIELD saga against "
            "the live byomd: two INERT bindings, then the owner CAS "
            "none -> byom at the expected revision; the exact retry "
            "returns the identical binding (crash-safe), an OVERLAPPING "
            "scope selector is ATTEMPTED and refused by the overlap rule, "
            "and a re-enable carrying CHANGED members cannot "
            "re-configure the committed mapping — nothing leaves a second "
            "binding or moves the owner",
            binding_ref=enabled["binding"]["binding_ref"],
            governance_owner=enabled["owner_binding"]["governance_owner"],
            owner_revision=enabled["owner_binding"]["revision"],
            state=enabled["state"],
            refused_overlap=enabled["refused_overlap"],
            reconfigure_ignored=enabled["reconfigure_ignored"])

    # 4. byomd configured with the inert host binding Kovee derived.
    host = install_host_binding(driver, byom, enabled, ev)
    ctx["binding_ref"] = host["binding_ref"]
    ev.step("kovee -> byom: the inert host-binding document derived by "
            "KOVEE's own hostint from the committed binding+mapping; "
            "byomd restarted, same incarnation, recovery-workload token "
            "published (amendment A2: configure and bind, never author)",
            binding_ref=host["binding_ref"], issuers=host["issuers"])

    # 5. Onboarding: offer -> the candidate's own acceptance -> TWO
    #    governance admissions.
    onboarding = onboard_agent(byom, society, genesis, tag, ev)
    ctx.update(onboarding)
    agent = (agent_factory(byom, society, ev, genesis) if agent_factory
             else AgentChannel(byom, society, ev))
    ctx["agent"] = agent
    ev.step("byom: membership_offer (governance) -> membership_accept "
            "(byom-mcp CANDIDATE profile) -> participant_admit + "
            "manifestation_admit (TWO governance decisions) — Standing "
            "active, participant channel minted",
            offer_id=onboarding["offer"],
            manifestation_ref=onboarding["manifestation"],
            participant_binding_epoch=onboarding["binding_epoch"])

    # 6. The bounded Mandate, then the exploration stream.
    mandate = issue_mandate(byom, agent, tag)
    ctx.update(mandate=mandate["mandate"],
               mandate_subject=mandate["subject_digest"])
    opened = agent.one("byom_activity_open", {
        "kind": "exploration", "purpose_ref": "purpose-explore-i1",
        "purpose_digest": digest(0xC0), "mandate_refs": [mandate["mandate"]],
        "budget_account_set_ref": PARENT_ACCOUNT})
    stream = opened["result"]["activity_stream_id"]
    ctx["stream"] = stream
    ev.step("byom: mandate chain (prepare over byom-mcp PARTICIPANT, seat "
            "position + issue on the direct human channel, budget "
            "reserved) and activity_open kind=exploration — the Mandate "
            "bounds the model_egress act class",
            mandate_id=mandate["mandate"], activity_stream=stream,
            allowed_act_classes=["model_egress"],
            reserved=byom.ledger()["reserved"])

    # 7. NOTIFICATION IS NOT A WAKE. The notice commits as evidence and
    #    creates nothing — asserted from byom's own records.
    before = activation_rows(byom)
    need(before == {"wake_intents": 0, "activation_admissions": 0,
                    "resource_allocations": 0, "episodes": 0},
         f"nothing has woken yet: {before}")
    notice = attention_notice(driver, stream, ctx["event"], f"{tag}-n1")
    r = notice["notice"]
    need(r["eligibility_effect"] == "no_effect", f"notice: {notice}")
    need(all(r["created"][k] is False for k in
             ("wake_intent", "activation_admission", "resource_allocation",
              "episode")),
         f"a notice creates nothing: {notice}")
    need(len(r["required_stages"]) == 6,
         f"the notice names the stages still required: {notice}")
    after = activation_rows(byom)
    need(after == before,
         f"a notification alone creates no admission, no allocation and "
         f"no episode: {after}")
    replayed = attention_notice(driver, stream, ctx["event"], f"{tag}-n1")
    need(replayed == notice, "the exact retry replays byte-identically")
    need(byom.count("SELECT COUNT(*) FROM attention_notices") == 1,
         "a replay commits no second notice")
    # An event kovee has NOT committed is not notifiable: kovee's own
    # sender refuses before byom is touched (R3-I01 f).
    invented = driver.problem("attention-notice", {
        "activity_stream_ref": stream, "generation": 1,
        "source_event_ref": "kvevt-not-in-koveed-ledger",
        "stable_notice_key": f"notice-{tag}-invented"})
    need("ledger" in json.dumps(invented),
         f"kovee must refuse to notify byom of an event it never committed: "
         f"{invented}")
    need(byom.count("SELECT COUNT(*) FROM attention_notices") == 1,
         "the refused notice reached no byom record")
    ev.step("kovee -> byom: attention_notice_record, SENT BY KOVEE's own "
            "byom client on byomd's narrow attention channel, carrying "
            "kovee's own committed event id and a source digest kovee "
            "derived — eligibility_effect=no_effect, created.{wake_intent,"
            "activation_admission,resource_allocation,episode} all false, "
            "byom's four activation tables still EMPTY (NOTIFICATION IS "
            "NOT A WAKE, L25), the exact retry replays byte-identically, "
            "and an event NOT in koveed's ledger is refused by kovee "
            "before byom is touched. HONEST LIMIT: kovee's "
            "`kovee-attention` crate is a two-line stub, so no "
            "AttentionContract subsystem DECIDES to notify — the trigger "
            "is the scenario's, the sender and every record are not",
            kovee_event=ctx["event"]["event_id"],
            sender=notice["sender"],
            activation_rows=after,
            required_stages=r["required_stages"],
            replay_byte_identical=True,
            uncommitted_event_refused=True)

    # 8. The participant's OWN wake intent, citing that exact cause; then
    #    a second notice, whose AT-MOST effect is eligibility.
    wake = agent.one("byom_wake_intent_submit", {
        "activity_stream_ref": stream, "generation": 1,
        "origin": "direct_participant",
        "exact_cause_ref": ctx["event"]["event_id"],
        "exact_cause_digest": keyed_of(ctx["event"]["event_id"]),
        "purpose_ref": "purpose-explore-i1",
        "stable_wake_key": f"wake-{tag}-1", "expires_at": FAR_FUTURE})
    wake_id = wake["result"]["wake_intent_id"]
    need(wake["result"]["state"] == "submitted", f"wake: {wake}")
    ctx["wake"] = wake_id
    second = attention_notice(driver, stream, ctx["event"], f"{tag}-n2")
    effect = second["notice"]["eligibility_effect"]
    need(effect in ("no_effect", "wake_intent_eligible"),
         f"notice effect: {second}")
    rows = activation_rows(byom)
    need(rows == {"wake_intents": 1, "activation_admissions": 0,
                  "resource_allocations": 0, "episodes": 0},
         f"eligibility is not admission: {rows}")
    ev.step("byom: wake_intent_submit (byom-mcp PARTICIPANT profile) "
            "citing kovee's exact event as its cause — the wake is the "
            "PARTICIPANT's; a second notice's at-most effect is "
            "eligibility, and still no admission, allocation or episode",
            wake_intent=wake_id, state="submitted",
            second_notice_effect=effect, activation_rows=rows)

    # 9. episode_request: byom's kernel stages 2 and 3, BEFORE any
    #    placement (family contract L25/A8).
    requested = agent.one("byom_episode_request", {
        "activity_stream_ref": stream, "generation": 1,
        "wake_intent_ref": wake_id,
        "activation_admission_ref": f"adm-{wake_id}-r1"},
        "byom-mcp-participant-frames.jsonl")
    res = requested["result"]
    episode = res["episode_id"]
    allocation = res["resource_allocation_id"]
    allocation_digest = res["resource_allocation_digest"]
    # R3-L02: the same reply PUBLISHES the frozen parent-budget fragment, so
    # nothing downstream names `rset-…`/`bridge-…`/`sub-…` by convention or
    # takes the parent account and worst case from a caller argument.
    parent_budget = res["parent_budget"]
    need(res["state"] == "eligible",
         f"the Episode is eligible but NOT queued: {res}")
    need(allocation_digest["class"] == "portable_public",
         f"the published allocation pin is cross-boundary: {res}")
    need(parent_budget["digest"]["class"] == "portable_public",
         f"the parent-budget fragment is cross-boundary: {parent_budget}")
    need(sorted(k for k in parent_budget if k != "digest") == sorted([
             "byom_budget_reservation_set_ref",
             "byom_budget_reservation_set_revision",
             "byom_budget_reservation_set_digest",
             "external_budget_bridge_ref",
             "external_budget_bridge_revision",
             "stable_external_reservation_key",
             "items"]),
         f"the fragment is byom's FROZEN member set: {parent_budget}")
    need(parent_budget["external_budget_bridge_ref"]
         == f"bridge-{allocation}"
         and parent_budget["items"][0]["worst_case_amount"] == WORST_CASE
         and parent_budget["items"][0]["account_ref"] == PARENT_ACCOUNT,
         f"the fragment publishes byom's real parent facts: {parent_budget}")
    need(byom.row("SELECT state FROM activation_admissions"
                  " WHERE admission_id = ?", f"adm-{wake_id}-r1")
         == "admitted", "stage 2 committed admitted")
    need(byom.row("SELECT state FROM resource_allocations"
                  " WHERE allocation_id = ?", allocation) == "reserved",
         "stage 3 committed reserved")
    need(byom.row("SELECT state FROM external_budget_bridges"
                  " WHERE bridge_id = ?", f"bridge-{allocation}")
         == "requested", "the §11.4 bridge is persisted before queueing")
    need(byom.row("SELECT state FROM episodes WHERE episode_id = ?",
                  episode) == "eligible", "not queued without placement")
    ctx.update(episode=episode, allocation=allocation)
    ev.step("byom: episode_request (participant) — the kernel's stage 2 "
            "ActivationAdmission and stage 3 ResourceAllocation commit "
            "INSIDE it, the §11.4 bridge is persisted, and the Episode is "
            "eligible but NOT queued: request comes BEFORE placement "
            "(family contract L25/A8)",
            episode=episode, admission=f"adm-{wake_id}-r1",
            allocation=allocation,
            allocation_digest_class=allocation_digest["class"],
            episode_state="eligible",
            reserved=byom.ledger()["reserved"])

    # 10. Stage 4 and the lease: Kovee's PlacementBinding, byom's narrow
    #     adapter, then claim/start under DUAL fences.
    activated = driver.ok("episode-activate", {
        "society_ref": society, "recovery_epoch": 0,
        "participant_ref": AGENT,
        "participant_binding_epoch": onboarding["binding_epoch"],
        "manifestation_ref": onboarding["manifestation"],
        "activity_stream_ref": stream, "generation": 1,
        "wake_intent_ref": wake_id,
        "kovee_invocation_ref": f"kovee-inv-{tag}",
        "context_manifest_ref": "kovee-ctxman-i1",
        "lease_ttl_seconds": 600,
        # No `parent_account_ref` and no `worst_case_amount`: the parent
        # travels ONLY as byom's frozen fragment, which kovee verifies
        # (R3-L02, disposition D-R3-3).
        "requested": {"episode_ref": episode, "generation": 1,
                      "state": res["state"],
                      "resource_allocation_ref": allocation,
                      "resource_allocation_digest": allocation_digest,
                      "parent_budget": parent_budget}})
    bound = activated["bound"]
    need(activated["admitted"]["episode_queued"] is True,
         f"the placement must queue the Episode: {activated}")
    need(activated["admitted"]["bridge_state"] == "confirmed",
         f"the subordinate reservation confirms the bridge: {activated}")
    # R3-L02: kovee CONSUMED and VERIFIED byom's fragment — the digest it
    # re-derived is the one byom published, and the parent facts it holds are
    # byom's own.
    verified = activated["admitted"]["verified_parent"]
    need(verified["fragment_digest"]["value_hex"]
         == parent_budget["digest"]["value_hex"],
         f"kovee verified the exact fragment byom published: {verified}")
    need(verified["external_budget_bridge_ref"]
         == parent_budget["external_budget_bridge_ref"]
         and verified["stable_external_reservation_key"]
         == parent_budget["stable_external_reservation_key"]
         and verified["parent_worst_case_amount"] == WORST_CASE,
         f"and holds byom's parent facts, not reconstructed ones: {verified}")
    # R3-U03: kovee's OWN capacity ledger, asserted on the ACCOUNT.
    cap = activated["admitted"]["kovee_capacity_account"]
    need(cap and cap["conserves"] and cap["reserved"] == WORST_CASE // 2,
         f"kovee debited its own capacity account, narrowed to half the "
         f"parent: {cap}")
    kv_sub = kovee.query(
        "SELECT state, charged, released_lifetime"
        " FROM byom_subordinate_reservations"
        " WHERE stable_external_reservation_key = ?",
        (parent_budget["stable_external_reservation_key"],))
    need(len(kv_sub) == 1 and kv_sub[0]["state"] == "confirmed",
         f"kovee's subordinate reservation is confirmed: {kv_sub}")
    need(byom.row("SELECT state FROM episodes WHERE episode_id = ?",
                  episode) == "running",
         "the Episode runs once claimed and started")
    need(byom.row("SELECT state FROM resource_allocations"
                  " WHERE allocation_id = ?", allocation) == "bridged",
         "stage 3 completes only with BOTH reservation sets")
    kv_binding = kovee.query(
        "SELECT state, episode_ref, byom_fence_epoch, kovee_invocation_fence"
        " FROM byom_episode_bindings WHERE stable_binding_key = ?",
        (bound["stable_binding_key"],))
    need(len(kv_binding) == 1 and kv_binding[0]["state"] == "bound",
         f"kovee's own binding row: {kv_binding}")
    need(kv_binding[0]["byom_fence_epoch"] == bound["byom_fence_epoch"]
         and kv_binding[0]["kovee_invocation_fence"]
         == bound["kovee_invocation_fence"],
         "both daemons hold the same fence pair")
    ctx.update(bound=bound, placement=activated["placement"],
               parent_budget=parent_budget)
    ev.step("kovee -> byom: PlacementBinding (the ONE activation record "
            "Kovee owns) -> placement_admit with the byom_subordinate "
            "reservation (narrowed, never above parent) -> episode_claim "
            "+ episode_start; the Episode is queued then running, the "
            "allocation is bridged, and BOTH daemons hold the same DUAL "
            "fence pair — verified from each daemon's own row",
            episode=episode, placement=activated["placement"]["placement_id"],
            byom_episode_state="running", byom_allocation_state="bridged",
            kovee_binding_state="bound",
            fences={"byom": bound["byom_fence_epoch"],
                    "kovee": bound["kovee_invocation_fence"]},
            subordinate=activated["admitted"]
                                 ["subordinate_reservation_ref"])

    if stop_after == "activation":
        return ctx

    # 11. The formation saga: exactly one Endeavor, formed by byom.
    formation = form_endeavor(kovee, byom, ctx, tag, ev)
    ctx.update(formation)

    # 12. The call and the pledge, with the full seat sequence.
    pledge = make_pledge(byom, agent, ctx, tag, ev)
    ctx.update(pledge)

    return ctx


def close_the_loop(ctx: dict, ev: Evidence, tag: str, transport: dict,
                   prompt: str) -> dict:
    """The act chain, the broker, the delivery and the terminalization —
    the second half of the loop, on the state `governed_setup` left."""
    byom, kovee, driver = ctx["byom"], ctx["kovee"], ctx["driver"]
    agent = ctx["agent"]
    episode, allocation = ctx["episode"], ctx["allocation"]

    # 13. The model_egress act chain to a consumed one-shot permit, and
    #     Kovee's broker behind it.
    broker = broker_call(kovee, byom, driver, agent, ctx, tag, ev,
                         transport, prompt)
    ctx.update(broker)

    # 14. The delivery the Pledge owes, and the beneficiary's review.
    ctx.update(deliver(byom, agent, ctx, tag, ev))

    # 15. Terminalize: the Episode completes and the reservation releases.
    completed = driver.ok("episode-complete",
                          {"stable_binding_key":
                               ctx["bound"]["stable_binding_key"]})
    ledger = byom.ledger()
    need(byom.row("SELECT state FROM episodes WHERE episode_id = ?",
                  episode) == "completed", "the Episode terminalizes")
    need(byom.row("SELECT state FROM external_budget_bridges"
                  " WHERE bridge_id = ?", f"bridge-{allocation}")
         == "released", "the bridge releases with the Episode")
    need(ledger["conserves"], f"conservation must hold: {ledger}")
    # Committed is accounted for, unit by unit and by name: the act's own
    # reservation (spent at the permit consumption) plus the metered charge
    # byom settled — and nothing else took a unit, before or after the
    # release.
    need(ledger["committed"] == ctx["act_committed"] + ctx["charged"],
         f"committed = the act's reservation + the metered charge: "
         f"{ledger}")
    live = [r for r in byom.reservations()
            if r["holder_kind"] == "episode_allocation"]
    need(all(r["state"] != "reserved" for r in live),
         f"the Episode's own reservation no longer holds units: {live}")
    ev.step("byom: episode_complete (runtime, under both fences) — the "
            "Episode terminalizes, the §11.4 bridge releases, the "
            "unspent reserve returns, and the conservation identity holds "
            "with committed = the act's own reservation + the METERED "
            "charge and nothing else",
            episode_state="completed", bridge_state="released",
            ledger=ledger, act_reservation=ctx["act_committed"],
            metered_charge=ctx["charged"],
            reservations=byom.reservations(),
            episode_complete=completed["episode_complete"].get("state"))

    return ctx


def scripted_flow(ev: Evidence, tag: str, transport: dict,
                  provider_env: dict, prompt: str,
                  agent_factory=None) -> dict:
    """The whole I1 loop: the governed setup, then the model call through
    the disclosed broker, the delivery and the terminalization."""
    ctx = governed_setup(ev, tag, provider_env, agent_factory)
    return close_the_loop(ctx, ev, tag, transport, prompt)


def deliver(byom: ByomDaemon, agent, ctx: dict, tag: str,
            ev: Evidence) -> dict:
    """delivery_submit (the pledgor, and only the pledgor) then
    review_record (the governing subject's reviewer) — the Pledge closes
    fulfilled."""
    inc = byom.incarnation()
    work = agent.one("byom_activity_open", {
        "kind": "pledge_work", "purpose_ref": "purpose-improve-i1",
        "purpose_digest": digest(0xD4),
        "pledge_binding": {"pledge_id": ctx["pledge"], "pledge_revision": 1,
                           "terms_digest": ctx["terms"]},
        "mandate_refs": [],
        "budget_account_set_ref": f"budget-endeavor-{tag}"})
    work_stream = work["result"]["activity_stream_id"]
    delivered = agent.one("byom_delivery_submit", {
        "pledge_id": ctx["pledge"], "pledge_revision": 2,
        "terms_digest": ctx["terms"],
        "output_refs": ["change-set-1"],
        "evidence_refs": [f"kovee-effect-{ctx['effect']['effect_id']}"],
        "activity_stream_ref": work_stream})
    delivery = delivered["result"]["delivery_id"]
    reviewed = byom.expect_ok("participant", {
        "version": "0.2", "op": "review_record",
        "meta": meta(inc, f"{tag}-review"),
        "pledge_id": ctx["pledge"],
        "pledge_revision": int(byom.row(
            "SELECT revision FROM pledges WHERE pledge_id = ?",
            ctx["pledge"])),
        "delivery_id": delivery,
        "reviewed_subject_digest": json.loads(byom.row(
            "SELECT subject_digest FROM deliveries WHERE delivery_id = ?",
            delivery)),
        "outcome": "fulfilled",
        "decision_or_mandate_use_ref": "dec-review-1"})
    need(reviewed["result"]["pledge_state"] == "fulfilled",
         f"review: {reviewed}")
    ev.step("byom: delivery_submit (the PLEDGOR, over its own channel, "
            "citing the kovee Effect as its evidence) -> review_record "
            "(the beneficiary, direct human channel) — the Pledge is "
            "fulfilled",
            work_stream=work_stream, delivery_id=delivery,
            evidence=[f"kovee-effect-{ctx['effect']['effect_id']}"],
            review_id=reviewed["result"]["review_id"],
            pledge_state="fulfilled")
    return {"delivery": delivery, "work_stream": work_stream}


def form_endeavor(kovee: Koveed, byom: ByomDaemon, ctx: dict, tag: str,
                  ev: Evidence) -> dict:
    """`kovee_endeavor_form` through the EndeavorFormationIntent/Slot/
    Attempt saga (byom §16.3, R39): kovee prepares locally, then ONE
    delegated-principal attempt forms exactly one Endeavor on byom's
    side. It never bootstraps: the active Society, Standing, recovery
    epoch and realm binding are all required."""
    frontier, assembly, formation = formation_prepare(kovee, byom, ctx, tag)
    prepared_state = kovee.expect_ok(kv(
        "endeavor_promotion_show", None, None,
        {"formation_id": formation}))["result"]
    need(prepared_state["state"] == "prepared",
         f"prepare: {prepared_state}")
    need(prepared_state["slot"]["state"] == "held",
         f"the formation slot is held: {prepared_state}")
    # R3-I04: this line used to be `need(True, "")` — an assertion that
    # asserted nothing, standing exactly where the saga's adversarial
    # checks belong. Three of them, against the live daemons, all before
    # the one legitimate attempt:
    #   1. a formation cannot BOOTSTRAP a Society: prepared against a
    #      society byomd does not know, it is refused;
    #   2. nor under a stale participant binding epoch;
    #   3. `start` on a formation id that does not exist is refused, so an
    #      external link is never minted from a name.
    epoch = int(byom.row("SELECT binding_epoch FROM participants"
                         " WHERE participant_id = ?", ctx["sovereign"]) or 1)
    def prepare_variant(key: str, society: str, binding_epoch: int) -> dict:
        return kovee.call(kv(
            "endeavor_promotion_prepare", None, f"idem-i1-prep-{key}",
            {"byom_endpoint_ref": "local", "society_ref": society,
             "frontier_ref": frontier,
             "collaboration_context_bundle_ref": assembly,
             "bound_participant_ref": ctx["sovereign"],
             "participant_binding_epoch": binding_epoch,
             "client_formation_key": f"form-key-{tag}-{key}",
             "endeavor_proposal_ref": f"prop-i1-{key}",
             "endeavor_proposal": {
                 "purpose_ref": "purpose-improve-i1",
                 "purpose_digest": portable(0xE1),
                 "sponsor_participant_refs": [ctx["sovereign"]],
                 "governance_rule_set_ref": "rules-endeavor-i1",
                 "outcome_schema_refs": ["schema-change-set-1"],
                 "acceptance_rule_ref": "rule-accept-1",
                 "classification_join_ref": "class-join-1",
                 "budget_account_set_ref": f"budget-endeavor-{tag}"},
             "source_principal_position": {
                 "participant_ref": ctx["sovereign"], "value": "assent",
                 "assent_mode": "direct_participant"}}))
    foreign = prepare_variant("foreign-society", "soc-not-this-installation",
                              epoch)
    need(foreign.get("outcome") != "ok",
         f"a formation must not prepare against a Society byomd does not "
         f"know: {foreign}")
    # A stale participant binding epoch passes PREPARE — prepare makes no
    # external contact at all, which is the property the step below
    # asserts — and is then refused by BYOM at `start`, where the
    # delegated-principal attempt actually reaches the Society.
    stale_epoch = prepare_variant("stale-epoch", ctx["society"], epoch + 7)
    need(stale_epoch.get("outcome") == "ok",
         f"prepare is local, so a stale epoch cannot be caught here: "
         f"{stale_epoch}")
    stale_start = kovee.call(kv(
        "endeavor_promotion_start", None, "idem-i1-start-stale",
        {"formation_id": stale_epoch["result"]["formation_id"],
         "authentication_observation_ref": f"authobs-{tag}-stale"}))
    need(stale_start.get("outcome") != "ok",
         f"byom must refuse a formation attempt under a stale participant "
         f"binding epoch: {stale_start}")
    need(byom.count("SELECT COUNT(*) FROM endeavors") == 0,
         "the refused attempt formed no Endeavor")
    unknown = kovee.call(kv("endeavor_promotion_start", None,
                            "idem-i1-start-unknown",
                            {"formation_id": "efi-not-a-formation",
                             "authentication_observation_ref":
                                 f"authobs-{tag}-x"}))
    need(unknown.get("outcome") != "ok",
         f"`start` on a formation that does not exist must be refused: "
         f"{unknown}")
    need(byom.count("SELECT COUNT(*) FROM endeavors") == 0,
         "no refusal may form an Endeavor")
    ctx["formation_refusals"] = {
        "prepare_foreign_society":
            (foreign.get("problem") or {}).get("type"),
        "start_under_stale_binding_epoch":
            (stale_start.get("problem") or {}).get("type"),
        "start_unknown_formation": (unknown.get("problem") or {}).get("type")}
    # prepare makes NO external contact: byom has no Endeavor yet.
    snap = byom.expect_ok("projection", {
        "version": "0.2", "op": "snapshot_get",
        "society_id": ctx["society"], "kinds": ["endeavors"]})
    need(not snap["result"]["endeavors"],
         "prepare must not contact byomd")
    return form_endeavor_start(kovee, byom, ctx, tag, ev, formation,
                               frontier, assembly)


def formation_prepare(kovee: Koveed, byom: ByomDaemon, ctx: dict,
                      tag: str) -> tuple[str, str, str]:
    """The kovee half, entirely local: a ContextAssembly at a pinned
    frontier, then the EndeavorFormationIntent and its held slot. No
    external contact happens here."""
    project, space, branch = ctx["project"], ctx["space"], ctx["branch"]
    assembled = kovee.expect_ok(kv(
        "context_assembly_create", project, "idem-i1-assembly",
        {"space_id": space, "branch_id": branch,
         "audience_ref": "asstdep-dep-local-dev",
         "purpose": "endeavor promotion",
         "selection_policy_ref": "explicit_refs_v1",
         "required_refs": [ctx["question"]],
         "trigger_refs": [ctx["question"]]}))
    assembly = assembled["result"]["assembly_id"]
    frontier = assembled["result"]["frontier_ref"]
    prepared = kovee.expect_ok(kv(
        "endeavor_promotion_prepare", None, "idem-i1-prepare",
        {"byom_endpoint_ref": "local", "society_ref": ctx["society"],
         "frontier_ref": frontier,
         "collaboration_context_bundle_ref": assembly,
         "bound_participant_ref": ctx["sovereign"],
         "participant_binding_epoch": int(
             byom.row("SELECT binding_epoch FROM participants"
                      " WHERE participant_id = ?", ctx["sovereign"]) or 1),
         "client_formation_key": f"form-key-{tag}",
         "endeavor_proposal_ref": "prop-i1-1",
         "endeavor_proposal": {
             "purpose_ref": "purpose-improve-i1",
             "purpose_digest": portable(0xE1),
             "sponsor_participant_refs": [ctx["sovereign"]],
             "governance_rule_set_ref": "rules-endeavor-i1",
             "outcome_schema_refs": ["schema-change-set-1"],
             "acceptance_rule_ref": "rule-accept-1",
             "classification_join_ref": "class-join-1",
             "budget_account_set_ref": f"budget-endeavor-{tag}"},
         "source_principal_position": {
             "participant_ref": ctx["sovereign"], "value": "assent",
             "assent_mode": "direct_participant"}}))
    formation = prepared["result"]["formation_id"]
    need(prepared["result"]["state"] == "prepared", f"prepare: {prepared}")
    return frontier, assembly, formation


def form_endeavor_start(kovee: Koveed, byom: ByomDaemon, ctx: dict,
                        tag: str, ev: Evidence, formation: str,
                        frontier: str, assembly: str) -> dict:
    """The ONE delegated-principal attempt: byom forms exactly one
    Endeavor, the slot releases, and every replay is byte-identical."""
    started = kovee.expect_ok(kv(
        "endeavor_promotion_start", None, "idem-i1-start",
        {"formation_id": formation,
         "authentication_observation_ref": f"authobs-{tag}-1"}))
    view = started["result"]
    need(view["state"] == "linked", f"start: {view}")
    need(view["slot"]["state"] == "released", f"slot after link: {view}")
    endeavor = view["external_link"]["endeavor_ref"]
    formed = byom.expect_ok("projection", {
        "version": "0.2", "op": "snapshot_get",
        "society_id": ctx["society"], "kinds": ["endeavors"]})["result"]
    need(len(formed["endeavors"]) == 1
         and formed["endeavors"][0]["endeavor_id"] == endeavor
         and formed["endeavors"][0]["state"] == "active",
         f"byomd formed exactly ONE active Endeavor: {formed}")
    need(byom.count("SELECT COUNT(*) FROM endeavors") == 1,
         "exactly one endeavor row in byom's own store")
    # A retry of the whole start is byte-identical, and still one Endeavor.
    a = kovee.call_raw(kv("endeavor_promotion_start", None, "idem-i1-start",
                          {"formation_id": formation,
                           "authentication_observation_ref":
                               f"authobs-{tag}-1"}))
    b = kovee.call_raw(kv("endeavor_promotion_start", None, "idem-i1-start",
                          {"formation_id": formation,
                           "authentication_observation_ref":
                               f"authobs-{tag}-1"}))
    need(a == b, "the exact retry of a formation is byte-identical")
    need(byom.count("SELECT COUNT(*) FROM endeavors") == 1,
         "a replayed formation forms nothing new")
    ev.step("kovee -> byom: endeavor_promotion_prepare (local, no "
            "contact) -> endeavor_promotion_start -> byom's "
            "kovee_endeavor_form: EXACTLY ONE active Endeavor, the slot "
            "released, and a byte-identical replay that forms nothing "
            "new — Position + GovernanceDecision + Endeavor as one "
            "atomic result. Attempted and refused FIRST: a prepare "
            "against a Society byomd does not know, a START under a "
            "stale participant binding epoch (refused by BYOM, since "
            "prepare is local), and a `start` on a "
            "formation id that does not exist — a formation never "
            "bootstraps anything",
            formation_id=formation, endeavor=endeavor,
            frontier=frontier, assembly=assembly,
            byom_endeavor_rows=1,
            refusals=ctx.get("formation_refusals"))
    return {"endeavor": endeavor, "formation": formation,
            "assembly": assembly, "frontier": frontier}


def make_pledge(byom: ByomDaemon, agent: AgentChannel, ctx: dict, tag: str,
                ev: Evidence) -> dict:
    """The call the human opens, and the Pledge the agent proposes into
    it: every required seat positioned by its own owner, then a
    DETERMINISTIC finalize that supplies no seat."""
    inc = byom.incarnation()
    call_opened = byom.expect_ok("participant", {
        "version": "0.2", "op": "call_open",
        "meta": meta(inc, f"{tag}-call"),
        "endeavor_id": ctx["endeavor"],
        "requested_outcome_schema_refs": ["schema-change-set-1"],
        "acceptance_criteria_refs": ["criteria-review-1"],
        "evidence_requirements": []})
    call_id = call_opened["result"]["call_id"]
    mcp = agent.open()
    try:
        pprop = mcp.call_ok("byom_pledge_propose", {
            "endeavor_id": ctx["endeavor"], "call_ref": call_id,
            "proposed_pledgor_ref": AGENT,
            "beneficiary_ref": ctx["sovereign"],
            "exact_outcome_schema_refs": ["schema-change-set-1"],
            "acceptance_criteria_refs": ["criteria-review-1"],
            "evidence_requirements": [],
            "reviewer_rule_ref": "rule-beneficiary-reviews",
            "input_context_ref": "context-input-1",
            "input_context_digest": digest(0xD2),
            "budget_request_set": {"items": [
                {"dimension": "unit", "canonical_unit": "unit",
                 "scale": 0, "max": 16}]},
            "allowed_manifestation_selector": {
                "rules": [{"effect": "allow", "atoms": {}}]},
            "delegation_ceiling": {"allowed": False, "max_depth": 0,
                                   "max_children": 0},
            "deadline": FAR_FUTURE,
            "cancellation_terms": {"terms_ref": "terms-cancel-1",
                                   "terms_digest": digest(0xD3)},
            "dependency_refs": []})
        proposal = pprop["result"]["proposal_id"]
        terms = pprop["result"]["terms_digest"]
        slots = {s["kind"]: s["seat_refs"][0]
                 for s in pprop["result"]["required_slots"]}
        mcp.call_ok("byom_pledge_position", {
            "proposal_ref": proposal, "proposal_revision": 1,
            "subject_digest": terms, "seat_ref": slots["pledgor_assent"],
            "value": "assent", "assent_mode": "direct_participant"})
    finally:
        mcp.close()
    byom.expect_ok("participant", {
        "version": "0.2", "op": "pledge_position",
        "meta": meta(inc, f"{tag}-ppos-sov"),
        "proposal_ref": proposal, "proposal_revision": 1,
        "subject_digest": terms,
        "seat_ref": slots["beneficiary_assent"],
        "value": "assent", "assent_mode": "direct_participant"})
    finalized = agent.one("byom_pledge_finalize", {
        "proposal_ref": proposal, "proposal_revision": 1,
        "subject_digest": terms})
    pledge = finalized["result"]["pledge_id"]
    need(byom.count("SELECT COUNT(*) FROM pledges") == 1,
         "exactly one pledge")
    ev.step("byom: call_open (human sovereign, direct human channel) -> "
            "pledge_propose + PLEDGOR seat (agent, byom-mcp) -> "
            "BENEFICIARY seat (human) -> pledge_finalize (deterministic, "
            "supplies no seat) — one committed Pledge",
            call_id=call_id, proposal_id=proposal, pledge_id=pledge,
            seats=sorted(slots))
    return {"call": call_id, "proposal": proposal, "pledge": pledge,
            "terms": terms}


def worker_call(kovee: Koveed, ctx: dict, transport: dict, prompt: str,
                key: str) -> tuple[dict, dict]:
    """The worker attempt the model call is bound to, over kovee's REAL
    §10.6 path — `invocation_create` on the external surface,
    `invocation_claim` on the disjoint worker surface — and the request
    body a worker may express: a logical model profile, a purpose, a
    classification and text. No provider, host, header or credential is
    nameable here."""
    invocation = kovee.expect_ok(kv(
        "invocation_create", ctx["project"], f"idem-i1-inv-{key}",
        {"assistant_deployment_id": "dep-local-dev",
         "assistant_deployment_revision": 1,
         "space_id": ctx["space"],
         "context_assembly_ref": ctx["assembly"],
         "deadline": FAR_FUTURE}))
    invocation_id = invocation["result"]["invocation_id"]
    claimed = kovee.expect_ok(kv(
        "invocation_claim", None, f"idem-i1-claim-{key}",
        {"invocation_id": invocation_id}), worker=True)
    call_args = {
        "project": ctx["project"],
        "attempt_id": claimed["result"]["attempt_id"],
        "fence_epoch": claimed["result"]["fence_epoch"],
        "model_profile_ref": transport["model_profile_ref"],
        "purpose_ref": "purpose-explore-i1",
        "classification_ref": "class-public",
        "system": "Answer with OK.", "prompt": prompt,
        "max_output_tokens": 16,
        "stable_binding_key": ctx["bound"]["stable_binding_key"]}
    return call_args, {"invocation": invocation_id,
                       "attempt": claimed["result"]["attempt_id"],
                       "fence": claimed["result"]["fence_epoch"]}


def worker_model_complete(kovee: Koveed, ctx: dict, transport: dict,
                          prompt: str, call_args: dict,
                          authorization: dict, nonce: str = "") -> dict:
    """koveed's REAL worker-socket `model_complete` (R3-I02).

    The driver exists because kovee exposes the episode pipeline as library
    API only — but the model call is NOT in that set: koveed serves
    `model_complete` on its disjoint §23.3 worker socket, and that op owns
    the parsing, the worker attempt-binding authentication, the mutexed
    store and — decisively — the choice of egress. Driving the broker only
    through the driver meant the harness chose the wire.

    This is the op, on the live daemon, with exactly the members a worker
    may express: no provider, host, header, credential or transport is
    nameable here, and the daemon supplies the wire.

    R3-I02 is CLOSED here, both ways. The gate drives this op at the points
    where it must refuse BEFORE egress (no permit; a spent permit) — the
    whole authority chain, zero bytes — and it also drives it to COMPLETION
    (`daemon_completing_dispatch`), because koveed now offers a no-network
    egress in a `koveed/testing` build (`$KOVEE_TESTING_RECORDING_EGRESS`,
    `koveed/src/main.rs`). The daemon still chooses the wire; the gate only
    chooses which daemon to build."""
    return kovee.call({
        "version": "0.1", "op": "model_complete", "realm_id": REALM,
        "project_id": ctx["project"],
        # The idempotency key is the LOGICAL call: an exact retry replays
        # the retained receipt rather than dispatching twice. `nonce`
        # therefore says "this is a DIFFERENT call", which is what a
        # second dispatch of a spent permit has to be for the refusal to
        # be a refusal and not a replay.
        "meta": {"request_id":
                     f"req-worker-model-{call_args['attempt_id']}"
                     f"-r{authorization['act_revision']}{nonce}",
                 "idempotency_key":
                     f"idem-worker-model-{authorization['stable_execution_key']}"
                     f"-r{authorization['act_revision']}{nonce}"},
        "args": {
            "attempt_id": call_args["attempt_id"],
            "fence_epoch": call_args["fence_epoch"],
            "model_profile_ref": transport["model_profile_ref"],
            "purpose_ref": "purpose-explore-i1",
            "classification_ref": "class-public",
            "system": "Answer with OK.",
            "prompt": prompt,
            "max_output_tokens": 16,
            "stable_binding_key": ctx["bound"]["stable_binding_key"],
            "act_intent_ref": authorization["act_intent_ref"],
            "act_intent_digest": authorization["act_intent_digest"],
            "act_revision": authorization["act_revision"],
            "subject_digest": authorization["subject_digest"],
            # R3-A01: the HOST-owned ContextManifest pair the act's seats
            # assented to. byom compares both at consumption, so a worker
            # request that cannot name them cannot drive a governed call.
            "context_manifest_ref": authorization["context_manifest_ref"],
            "context_manifest_digest":
                authorization["context_manifest_digest"],
            "stable_execution_key": authorization["stable_execution_key"],
            "budget_reservation_set_ref":
                authorization["budget_reservation_set_ref"]}},
        worker=True)


def prepare_act(byom: ByomDaemon, agent, ctx: dict, staged: dict, tag: str,
                key: str) -> dict:
    """`act_intent_prepare` for the Δ4 `model_egress` class, over the
    disclosure kovee committed. The subject is COMPILED server-side; the
    caller supplies no atoms."""
    prepared = agent.one("byom_act_intent_prepare", {
        "kind": "model_egress", "execution_kind": "external_effect",
        "subject_ref": f"subject-{tag}-{key}-egress", "subject_revision": 1,
        "mandate_ref": ctx["mandate"],
        "mandate_revision": int(byom.row(
            "SELECT revision FROM mandates WHERE mandate_id = ?",
            ctx["mandate"]) or 1),
        "mandate_digest": ctx["mandate_subject"],
        "context_manifest_ref": "kovee-ctxman-i1",
        # A8 (byom's act_ops now pins the class): the ContextManifest is the
        # HOST's object and byom holds only its digest, so the pair travels
        # `portable_public`. Coordination note: this fixture value belongs to
        # the R3-L01/A8 work, not to the budget fix.
        "context_manifest_digest": portable(0xE1),
        "disclosure_manifest_ref": staged["disclosure_manifest_ref"],
        "disclosure_manifest_digest": staged["disclosure_manifest_digest"],
        "driver_audience": BROKER_AUDIENCE})
    return prepared["result"]


def act_authorization(byom: ByomDaemon, act: dict, revision: int) -> dict:
    """The NOTICE kovee echoes into `execution_permit_consume`. Every
    member is byom's own committed value — including the ActIntent record
    digest, which byom exposes on no wire surface, so it is read from
    byomd's own store beside the daemon.

    The CONTEXT pair (R3-A01) is the host's own binding, read back from
    byom's committed act rather than repeated from this file: byom compares
    both members against the subject its gate seat assented to, so an act
    can no longer execute under a context no seat ever saw."""
    row = byom.rows("SELECT intent_digest, context_manifest_ref,"
                    " context_manifest_digest FROM act_intents"
                    " WHERE intent_id = ?", (act["intent_id"],))[0]
    return {
        "act_intent_ref": act["intent_id"],
        "act_intent_digest": json.loads(row["intent_digest"]),
        "act_revision": revision,
        "subject_digest": act["subject_digest"],
        "context_manifest_ref": row["context_manifest_ref"],
        "context_manifest_digest": json.loads(row["context_manifest_digest"]),
        "stable_execution_key": act["stable_execution_key"],
        "budget_reservation_set_ref": act["budget_reservation_set_ref"]}


def authorize_act(byom: ByomDaemon, act: dict, tag: str,
                  key: str) -> tuple[dict, dict]:
    """The human GATE seat's Position, then ONE GovernanceDecision."""
    inc = byom.incarnation()
    positioned = byom.expect_ok("governance", {
        "version": "0.2", "op": "act_intent_position",
        "meta": meta(inc, f"{tag}-actpos-{key}"),
        "proposal_ref": act["intent_id"], "proposal_revision": 1,
        "subject_digest": act["subject_digest"],
        "seat_ref": act["required_seat_refs"][0], "value": "assent"})
    finalized = byom.expect_ok("governance", {
        "version": "0.2", "op": "act_intent_finalize",
        "meta": meta(inc, f"{tag}-actfin-{key}", 1),
        "intent_id": act["intent_id"],
        "subject_digest": act["subject_digest"]})
    return positioned, finalized


def broker_call(kovee: Koveed, byom: ByomDaemon, driver: Driver,
                agent: AgentChannel, ctx: dict, tag: str, ev: Evidence,
                transport: dict, prompt: str) -> dict:
    """The model_egress act chain and the broker behind it — the heart of
    I1.

    Order matters and is asserted: kovee STAGES the DisclosureManifest
    first (byom's act binds its exact digest), byom prepares/positions/
    finalizes the act, kovee's broker commits the Effect `prepared`, and
    only then does it consume byom's ONE-SHOT permit and dispatch. The
    refusal without a permit is exercised BEFORE the permit exists, on
    the same act."""
    inc = byom.incarnation()
    call_args, worker = worker_call(kovee, ctx, transport, prompt, "1")
    invocation_id, attempt, fence = (worker["invocation"], worker["attempt"],
                                     worker["fence"])

    # kovee's provider bindings, re-seeded from the environment the
    # scenario started: an absent key is recorded `disabled`, and the
    # deterministic modes carry a visible placeholder.
    seeded = driver.ok("seed-bindings", {})
    active = [b for b in seeded["bindings"] if b["status"] == "active"]
    need(any(b["binding_ref"] == transport["provider_binding_ref"]
             for b in active),
         f"the provider binding must be active to plan a call: {seeded}")

    # Step 1 of the chain: the §16.2 DisclosureManifest, COMMITTED, so
    # byom's act can bind its exact digest.
    staged = driver.ok("stage", call_args)
    claims = staged["provider_claims"]
    for member in ("region", "retention", "training_use"):
        need(claims.get(member),
             f"the disclosure manifest must claim {member}: {claims}")
    need(staged["recipient_binding"]
         == f"model-profile:{transport['model_profile_ref']}",
         "the disclosure names the EXACT model profile bytes leave "
         f"through, not a vendor: {staged}")
    # R3-I04: `exact_items` used to be printed and never checked. §16.2 is
    # about the FINAL BYTES, so this compares kovee's committed items with
    # the exact strings this scenario is sending — ref, digest and size,
    # item by item, plus the total.
    expect_items = [
        {"ref": f"{attempt}#system",
         "digest": hashlib.sha256(call_args["system"].encode()).hexdigest(),
         "size": len(call_args["system"].encode())},
        {"ref": f"{attempt}#prompt",
         "digest": hashlib.sha256(call_args["prompt"].encode()).hexdigest(),
         "size": len(call_args["prompt"].encode())}]
    got_items = [{"ref": i["ref"],
                  "digest": i["digest"]["value_hex"],
                  "size": i["size"]} for i in staged["exact_items"]]
    need(got_items == expect_items,
         f"the disclosure's exact_items must be the exact bytes that "
         f"leave: {got_items} vs {expect_items}")
    need(all(i["digest"]["class"] == "portable_public"
             for i in staged["exact_items"]),
         f"each item digest is re-derivable by byom: {staged['exact_items']}")
    need(staged["total_bytes"] == sum(i["size"] for i in expect_items),
         f"total_bytes is the sum of the exact items: {staged}")
    ev.step("kovee: the §16.2 DisclosureManifest is STAGED and COMMITTED "
            "before any authority is asked for — it names the exact "
            "model profile the bytes leave through, the provider's "
            "asserted {region, retention, training_use}, and exact_items "
            "that this run RE-DERIVED independently: ref, sha-256 and "
            "byte size per item and the total, over the very strings it "
            "is sending",
            disclosure=staged["disclosure_manifest_ref"],
            provider_claims=claims,
            recipient_binding=staged["recipient_binding"],
            model_selector=staged["model_selector"],
            invocation=invocation_id, attempt=attempt, fence_epoch=fence,
            exact_items=got_items,
            items_reproduced_independently=True,
            total_bytes=staged["total_bytes"])

    # byom's own act chain over kovee's committed disclosure.
    act = prepare_act(byom, agent, ctx, staged, tag, "1")
    intent = act["intent_id"]
    atoms = act["act_class_subject"]["subject_atoms"]
    need(sorted(atoms) == ["binding", "classification", "operation",
                           "purpose", "quantity"],
         f"the compiled Δ4 subject carries exactly its mandatory "
         f"domains: {sorted(atoms)}")
    need(atoms["binding"] == f"kovee:{BROKER_AUDIENCE}",
         f"the class subject pins the EXACT provider binding: {atoms}")
    authorization = act_authorization(byom, act, revision=1)
    ev.step("byom: act_intent_prepare (participant, byom-mcp) — the Δ4 "
            "model_egress class subject is COMPILED by the kernel over "
            "kovee's committed disclosure digest, with exactly its "
            "mandatory domains and the EXACT provider binding pinned",
            intent_id=intent, act_class=act["act_class"],
            subject_atoms=sorted(atoms), binding=atoms["binding"],
            stable_execution_key=act["stable_execution_key"])

    # REFUSAL: no permit, no egress. The act is prepared and nothing has
    # authorized it, so there is nothing to consume — and the Effect is
    # already COMMITTED `prepared` when the refusal happens.
    refused = driver.problem("complete", {
        **call_args, **transport["args"], "authorization": authorization})
    detail = str(refused.get("detail") or "")
    need("prepared" in detail,
         f"byom's own answer must name the act's state: {refused}")
    # R3-I03: the refusal path used to DROP the send count (the reply is a
    # problem, and the count only ever rode the ok reply). The driver now
    # writes it to a file this process reads, so "not one byte left" is a
    # number from outside the failing call, not an inference.
    refused_sends = driver.durable_sends()
    need(refused_sends == 0,
         f"a refused call must leave the transport untouched, and the "
         f"external counter says {refused_sends}")
    # The SAME refusal over koveed's REAL worker-socket `model_complete`
    # (R3-I02): the op koveed exposes, with koveed's own parsing,
    # authentication of the worker attempt binding, mutexed store and
    # DAEMON-CHOSEN egress. It refuses before the socket, so the
    # deterministic gate can drive the real op with no network at all.
    worker_refusal = worker_model_complete(kovee, ctx, transport, prompt,
                                           call_args, authorization)
    need(worker_refusal["outcome"] != "ok",
         f"koveed's own worker op must refuse an unauthorized act: "
         f"{worker_refusal}")
    need("prepared" in json.dumps(worker_refusal),
         f"and its refusal is byom's own answer about the act's state: "
         f"{worker_refusal}")
    effect = driver.ok("effect-show",
                       {"execution_key": act["stable_execution_key"]})
    need(effect["effect"]["state"] == "prepared",
         f"the Effect is committed prepared before any dispatch: {effect}")
    need(effect["attempts"] == [] and effect["consumptions"] == [],
         f"no attempt and no consumption without a permit: {effect}")
    need(byom.count("SELECT COUNT(*) FROM execution_consumption_receipts")
         == 0 and byom.count("SELECT COUNT(*) FROM mandate_uses") == 0,
         "byom minted no receipt and inserted no MandateUse")
    ev.step("kovee broker REFUSES without the permit — through the driver "
            "AND through koveed's own worker-socket `model_complete`: "
            "byom answers that the act is only `prepared`, the Effect row "
            "is already COMMITTED prepared (write order = the safety "
            "property), there is NO attempt, NO consumption, NO receipt "
            "and NO MandateUse, and the EXTERNAL durable send counter "
            "reads 0 — the refusal path no longer discards it",
            problem=refused.get("type"), detail=detail,
            durable_transport_sends=refused_sends,
            worker_socket_op="model_complete",
            worker_socket_refusal=worker_refusal.get("problem", {}).get(
                "type") or worker_refusal.get("outcome"),
            worker_socket_refusal_detail=str(
                worker_refusal.get("problem", {}).get("detail"))[:240],
            effect_state=effect["effect"]["state"],
            byom_receipts=0, byom_mandate_uses=0)

    # The human GATE seat assents, then ONE GovernanceDecision.
    positioned, finalized = authorize_act(byom, act, tag, "1")
    need(finalized["result"]["state"] == "authorized",
         f"finalize: {finalized}")
    need(finalized["result"]["authorization_decision_ref"]
         == f"dec-act-{intent}",
         f"ONE GovernanceDecision derived from the subject: {finalized}")
    authorization["act_revision"] = finalized["result"]["revision"]
    ev.step("byom: act_intent_position (eligible human GATE seat, fresh "
            "challenge, current digest) + act_intent_finalize "
            "(deterministic) — ONE GovernanceDecision bound to the exact "
            "subject digest",
            seat_ref=act["required_seat_refs"][0],
            decision=finalized["result"]["authorization_decision_ref"],
            act_state="authorized",
            act_revision=authorization["act_revision"],
            positioned_state=positioned["result"].get("state"))

    base_ledger = byom.ledger()
    completion = driver.ok("complete", {
        **call_args, **transport["args"],
        "authorization": authorization})
    need(completion["state"] == "completed",
         f"the authorized call completes: {completion}")
    need(completion["transport_profile"] == transport["profile"],
         f"the effect records the wire that carried it: {completion}")
    if transport.get("expect_send_count") is not None:
        need(completion["transport_send_count"]
             == transport["expect_send_count"],
             f"exactly one exchange: {completion}")
        need(driver.durable_sends() == transport["expect_send_count"],
             f"and the EXTERNAL counter agrees: {driver.last_sends}")
    usage = completion["usage"]

    # byom's side of the permit: ONE receipt, max_uses 1, ONE MandateUse.
    receipts = byom.count("SELECT COUNT(*) FROM"
                          " execution_consumption_receipts")
    uses = byom.count("SELECT COUNT(*) FROM mandate_uses")
    need(receipts == 1 and uses == 1,
         f"one consumption yields one receipt and one MandateUse: "
         f"{receipts}/{uses}")
    need(byom.row("SELECT state FROM act_intents WHERE intent_id = ?",
                  intent) == "consumed",
         "the one-shot act is spent")

    # kovee's side: the effect chain and the metering evidence.
    effect = driver.ok("effect-show",
                       {"execution_key": act["stable_execution_key"]})
    need(effect["effect"]["state"] == "completed",
         f"kovee's effect row: {effect}")
    need(len(effect["attempts"]) == 1
         and effect["attempts"][0]["state"] == "completed"
         and effect["attempts"][0]["transport_profile"]
         == transport["profile"],
         f"exactly one attempt, on the recorded wire: {effect}")
    need(len(effect["consumptions"]) == 1
         and effect["consumptions"][0]["owner_protocol"] == "byom"
         and effect["consumptions"][0]["phase"] == "pre_egress"
         and effect["consumptions"][0]["state"] == "spent",
         f"one pre-egress consumption of byom's permit: {effect}")
    need(len(effect["usage_reports"]) == 1, f"one usage report: {effect}")
    report = effect["usage_reports"][0]
    need(report["input_tokens"] == usage["input_tokens"]
         and report["output_tokens"] == usage["output_tokens"],
         f"kovee meters exactly what the provider reported: {report}")
    need(report["settled_by_byom"] is True,
         f"BYOM settles; kovee's row is evidence: {report}")
    charged = usage["input_tokens"] + usage["output_tokens"]

    # BYOM's own settlement — the record and the ledger move are byom's,
    # never kovee's. The measured quantities are the provider's counts and
    # the charge is their total, read out of byom's own settlement row.
    settlements = byom.rows(
        "SELECT stable_settlement_key, reservation_set_ref,"
        " measured_quantities, charged_quantities, status"
        " FROM usage_settlements")
    need(len(settlements) == 1,
         f"exactly one byom settlement: {settlements}")
    measured = {q["dimension"]: q["amount"]
                for q in json.loads(settlements[0]["measured_quantities"])}
    charged_q = json.loads(settlements[0]["charged_quantities"])
    need(measured.get("input_tokens") == usage["input_tokens"]
         and measured.get("output_tokens") == usage["output_tokens"],
         f"byom's settlement measured the provider's own token counts: "
         f"{measured} vs {usage}")
    need(charged_q == [{"dimension": "unit", "unit": "unit",
                        "amount": charged}],
         f"byom charged exactly the metered total: {charged_q}")
    need(settlements[0]["status"] == "measured",
         f"the settlement is a MEASURED settlement (a trusted meter's "
         f"measurement, not an estimate): {settlements}")
    need(settlements[0]["reservation_set_ref"]
         == f"rset-{ctx['allocation']}",
         f"the settlement charges the Episode's OWN reservation set: "
         f"{settlements}")
    reports = byom.rows("SELECT source, stable_report_key, quantities,"
                        " settlement_ref FROM usage_reports")
    need(len(reports) == 1 and reports[0]["source"] == "trusted_meter"
         and reports[0]["settlement_ref"],
         f"one TRUSTED-METER usage report, settled: {reports}")
    reported = {q["dimension"]: q["amount"]
                for q in json.loads(reports[0]["quantities"])}
    need(reported == measured,
         f"byom's report and settlement agree: {reported} vs {measured}")
    ledger = byom.ledger()
    need(ledger["conserves"], f"conservation holds: {ledger}")
    act_committed = [r["amount"] for r in byom.reservations(
        state="committed") if r["holder_kind"] == "act_intent"]
    need(len(act_committed) == 1,
         f"the act's own reservation is committed once: {act_committed}")
    need(ledger["committed"] == act_committed[0] + charged,
         f"the settlement moved exactly {charged} from reserved to "
         f"committed on top of the act's own reservation: {ledger}")
    need(ledger["reserved"] == base_ledger["reserved"]
         - act_committed[0] - charged,
         f"the charge left `reserved` for `committed`: {ledger} "
         f"(base {base_ledger})")

    # R3-U02: and KOVEE's own subordinate is settled by the SAME saga, across
    # the inter-daemon commit boundary. The defect was exactly this pair of
    # numbers disagreeing: byom charged the metered total while kovee stayed
    # `confirmed, charged = 0, released_lifetime = 0`.
    stable_key = ctx["parent_budget"]["stable_external_reservation_key"]
    kv_sub = kovee.query(
        "SELECT subordinate_reservation_ref, state, charged, released_lifetime"
        " FROM byom_subordinate_reservations"
        " WHERE stable_external_reservation_key = ?", (stable_key,))
    need(len(kv_sub) == 1 and kv_sub[0]["state"] == "settled",
         f"kovee's subordinate reservation is SETTLED, not left confirmed: "
         f"{kv_sub}")
    need(kv_sub[0]["charged"] == charged,
         f"kovee charged exactly what byom charged ({charged}): {kv_sub}")
    saga = kovee.query(
        "SELECT phase, charge, remote_charged FROM kovee_settlement_saga"
        " WHERE subordinate_reservation_ref = ?",
        (kv_sub[0]["subordinate_reservation_ref"],))
    need(len(saga) == 1 and saga[0]["phase"] == "settled"
         and saga[0]["charge"] == charged
         and saga[0]["remote_charged"] == charged,
         f"the two-sided saga record resolved to byom's own number: {saga}")
    kv_cap = kovee.query(
        "SELECT ceiling, remaining, reserved, committed, uncertain,"
        " delegated_to_children FROM kovee_capacity_accounts"
        " WHERE dimension = 'unit' AND account_ref = ?",
        (f"kovee-capacity-{REALM}",))
    need(len(kv_cap) == 1, f"kovee's capacity account exists: {kv_cap}")
    a = kv_cap[0]
    need(a["ceiling"] == a["remaining"] + a["reserved"] + a["committed"]
         + a["uncertain"] + a["delegated_to_children"],
         f"kovee's capacity ledger conserves: {a}")
    need(a["committed"] == charged,
         f"the charge left kovee's `reserved` for `committed`: {a}")

    # A SPENT one-shot permit REFUSES a second dispatch. R3-I04: this was
    # narrated as "the exact retry replays" — it is not a replay, it is a
    # refusal, and the difference is the whole point of a one-shot permit.
    # The durable counter proves the refusal happened before the wire.
    spent = driver.problem("complete", {
        **call_args, **transport["args"],
        "authorization": authorization})
    need("spent" in str(spent.get("detail") or ""),
         f"the one-shot permit must refuse a second dispatch: {spent}")
    spent_sends = driver.durable_sends()
    need(spent_sends == 0,
         f"the refused second dispatch sent nothing, and the external "
         f"counter says {spent_sends}")
    need(byom.count("SELECT COUNT(*) FROM mandate_uses") == 1,
         "a refused second dispatch never inserts a second MandateUse")
    after = driver.ok("effect-show",
                      {"execution_key": act["stable_execution_key"]})
    need(len(after["attempts"]) == len(effect["attempts"]),
         f"a refused second dispatch adds no attempt: {after}")
    # And the same refusal through koveed's OWN worker-socket op (R3-I02),
    # so the spent-permit gate is proven on the daemon's path too.
    worker_spent = worker_model_complete(kovee, ctx, transport, prompt,
                                         call_args, authorization)
    need(worker_spent["outcome"] != "ok",
         f"koveed's worker op must refuse a spent permit: {worker_spent}")
    need("spent" in json.dumps(worker_spent),
         f"and name it spent: {worker_spent}")
    need(byom.count("SELECT COUNT(*) FROM mandate_uses") == 1,
         "the worker-op refusal inserts no MandateUse either")
    # The model call is a WORKER operation and nothing else: the same
    # request on the external client surface is not even an operation
    # there (§23.3's disjoint surfaces, asserted rather than assumed).
    external = kovee.call({
        "version": "0.1", "op": "model_complete", "realm_id": REALM,
        "project_id": ctx["project"],
        "meta": {"request_id": "req-external-model",
                 "idempotency_key": "idem-external-model"},
        "args": {}})
    need(external.get("outcome") != "ok",
         f"model_complete must not exist on the external surface: "
         f"{external}")

    # The disclosure manifest, read from KOVEE's own read surface.
    shown = kovee.expect_ok(kv("disclosure_manifest_show", None, None,
                               {"disclosure_id":
                                    staged["disclosure_manifest_ref"]}))
    disclosed = shown["result"]
    for member in ("region", "retention", "training_use"):
        need(disclosed["provider_claims"].get(member),
             f"kovee's committed manifest carries {member}: {disclosed}")
    ev.step("kovee broker PROCEEDS with the permit: "
            "execution_permit_consume on byom's permit channel yields ONE "
            "receipt (max_uses 1) and ONE MandateUse; the effect goes "
            "prepared -> dispatching -> completed on the recorded wire; "
            "usage is metered back to byom and BYOM settles it. A second "
            "dispatch of the SPENT one-shot permit is REFUSED — not "
            "replayed: no second MandateUse, no second attempt, and the "
            "external send counter reads 0 for it — and koveed's own "
            "worker-socket `model_complete` refuses it the same way",
            effect=completion["effect_id"],
            transport_profile=completion["transport_profile"],
            usage=usage, charged=charged,
            byom_receipts=receipts, byom_mandate_uses=uses,
            byom_settlements=1, byom_ledger=ledger,
            kovee_usage_report=report["stable_report_key"],
            settled_by_byom=report["settled_by_byom"],
            byom_measured=measured, byom_charged=charged_q,
            second_dispatch_refused_as_spent=spent.get("type"),
            second_dispatch_durable_sends=spent_sends,
            worker_socket_spent_refusal=worker_spent.get(
                "problem", {}).get("type"),
            worker_socket_spent_detail=str(
                worker_spent.get("problem", {}).get("detail"))[:240],
            model_complete_absent_on_external_surface=True,
            disclosure_claims=disclosed["provider_claims"])
    return {"charged": charged, "usage": usage, "effect": completion,
            "act_committed": act_committed[0],
            "intent": intent, "disclosure": disclosed,
            "attempt": attempt, "fence": fence,
            "execution_key": act["stable_execution_key"],
            "invocation": invocation_id}


def per_source_trails(ctx: dict, ev: Evidence, label: str):
    """The two trails, asserted separately. byom's records carry byom's
    claims; kovee's records carry kovee's. No merged projection exists."""
    byom, kovee = ctx["byom"], ctx["kovee"]
    events = timeline(byom, ctx["genesis"])
    kinds = [e["kind"] for e in events]
    assert_ordered(kinds, BYOM_EXPECTED_ORDER, f"byom/{label}")
    table = verify_byom_attribution(events, ctx["sovereign"])
    ev.blob("byom-attribution.json", json.dumps(table, indent=1))
    ev.blob("byom-timeline.json", json.dumps(kinds, indent=1))

    # kovee's project stream, over kovee's own read surface.
    kev = kovee_events(kovee, ctx["project"])
    types = [e["type"] for e in kev]
    for i, e in enumerate(kev):
        need(e.get("project_sequence") == i + 1,
             f"kovee project sequences not dense at {i}: {e}")
        need(e.get("actor_ref"), f"kovee event without actor_ref: {e}")
    # kovee's REALM-scoped stream — the broker's own chain. §16.1: once the
    # broker has consumed the permit it records the outcome under its own
    # service identity, not the agent's. `events_read` serves project
    # streams only, so this reads koveed's own ledger table beside the
    # running daemon (the inspection channel kovee's K2 suites use).
    realm = kovee.query(
        "SELECT type, actor_ref FROM events WHERE project_id IS NULL"
        " AND type LIKE 'dev.kovee.model-effect%' ORDER BY stream_sequence")
    chain = [r["type"] for r in realm]
    assert_ordered(chain, ["dev.kovee.model-effect.prepared.v1",
                           "dev.kovee.model-effect.authorized.v1",
                           "dev.kovee.model-effect.dispatching.v1",
                           "dev.kovee.model-effect.completed.v1",
                           "dev.kovee.model-effect.usage-reported.v1"],
                   f"kovee-broker/{label}")
    need({r["actor_ref"] for r in realm} == {"svc-kovee-model-broker"},
         f"the broker records under its OWN service identity: {realm}")
    ev.blob("kovee-timeline.json", json.dumps(types, indent=1))
    ev.blob("kovee-broker-chain.json", json.dumps(realm, indent=1))
    ev.step("per-source trails: byom's events_read timeline holds the whole "
            "I1 arc in order, with EVERY kind checked against an "
            "exhaustive actor map (the kernel stages authored by the "
            "kernel, each kovee adapter on its own narrow channel, the "
            "formation by the delegated principal); kovee's project "
            "stream is dense and attributed and its broker chain is "
            "recorded under the broker's own service identity — asserted "
            "separately, never merged into one view",
            byom_events=len(events), byom_kinds=len(set(kinds)),
            kovee_project_events=len(kev),
            kovee_broker_chain=chain,
            broker_actor="svc-kovee-model-broker")


def cell_attribution(byom: ByomDaemon, genesis: str, sov: str,
                     ev: Evidence, label: str) -> list:
    """Every event a CELL produced, checked against the same exhaustive
    actor map the main flow uses — so the records the new cells introduce
    (the Continuation, the yield, both effect heads, the onboarding path)
    are attributed too, and an unmapped kind fails the gate."""
    events = timeline(byom, genesis)
    table = verify_byom_attribution(events, sov)
    ev.blob("byom-attribution.json", json.dumps(table, indent=1))
    ev.blob("byom-timeline.json",
            json.dumps([e["kind"] for e in events], indent=1))
    need(len({r["kind"] for r in table}) >= 1, f"{label}: no events")
    return table


def honesty_labels(ev: Evidence, transport: dict, note: str = ""):
    ev.step("assurance profile, labeled honestly: DEVELOPER — no UID "
            "separation, no attested process identity, no asymmetric "
            "endpoint identity. The gate claims only that the calls it "
            "EXERCISED went through the disclosed, metered broker; "
            "provider-bypass PREVENTION is NOT claimed until K4's secure "
            "profile. Data is synthetic and non-sensitive; no production "
            "effect is performed. And the koveed this gate runs is a TEST "
            "BUILD (`--features testing`): that is what gives the daemon a "
            "no-network wire to offer, which is what lets the gate drive "
            "koveed's own `model_complete` to completion (R3-I02). A "
            "production build compiles no such wire, and the daemon-egress "
            "cell checks that on the binaries rather than asserting it",
            assurance_profile="developer",
            bypass_prevention_claimed=False,
            confinement_claimed=False,
            data="synthetic, non-sensitive",
            koveed_build=f"cargo build -p koveed --features "
                         f"{KOVEE_TEST_FEATURE.split('/')[1]}",
            transport_profile=transport["profile"], note=note)


# ------------------------------------------- plan §8 I1: the extra cells ----

# The hosted Manifestation the successor attempt runs under. It is backed by
# KOVEE's own assistant deployment (`dep-local-dev`, revision 1, security
# profile `developer`, status `active` — a row in koveed's schema), which is
# what makes it a HOSTED Manifestation rather than the attached harness the
# offer proposed.
#
# HONEST LIMIT, asserted below rather than glossed: byom mints
# ManifestationRevisions ONLY inside `membership_offer`, which fixes
# `kind: attached_harness`, and there is no byom operation that admits a
# `host_kind: kovee_deployment` revision (`grep -rn kovee_deployment
# byom/crates` finds nothing but the DESIGN.md enum). `placement_admit` also
# does not resolve `selected_manifestation_ref` against that table. So the
# hosted Manifestation is exercised as far as the two daemons' surfaces
# reach: kovee SELECTS it at placement from its own deployment record, byom
# COMMITS it on the Episode and in the PlacementAdmission, and both are read
# back per source — while byom's own ManifestationRevision rows still say
# `attached_harness`, which the evidence states.
KOVEE_DEPLOYMENT = "dep-local-dev"
HOSTED_MANIFESTATION = f"manif-hosted-{KOVEE_DEPLOYMENT}"


def agent_refusal(agent: AgentChannel, tool: str, args: dict) -> str:
    """One agent-channel call that MUST be refused, with the refusal text
    returned so the cell can name it."""
    mcp = agent.open()
    try:
        text, is_error = mcp.call(tool, args)
        need(is_error, f"{tool} must be refused, it answered: {text}")
        return text
    finally:
        mcp.close()


def continuation_write(agent: AgentChannel, stream: str, episode: str,
                       fence: int, head: int, summary: str,
                       prior: dict | None = None) -> dict:
    body = {"activity_stream_ref": stream, "generation": 1,
            "summary_ref": summary, "unresolved_refs": [],
            "exact_state_refs": [f"state-{summary}"],
            "source_event_cursor": f"cursor-{summary}",
            "expected_head_revision": head,
            "classification_ref": "class-participant-private",
            "episode_ref": episode, "byom_fence_epoch": fence}
    if prior is not None:
        body["prior_continuation_ref"] = prior["continuation_id"]
        body["prior_continuation_digest"] = prior["digest"]
    return agent.one("byom_continuation_write", body)["result"]


def continuation_resume_cell(ev: Evidence) -> None:
    """Plan §8 I1: **cross-Manifestation Continuation resume**, live.

    One Episode writes the participant-owned Continuation and YIELDS; a
    DIFFERENT, hosted Manifestation resumes from that portable Continuation
    under a new Kovee invocation and a new byom fence, and advances the ONE
    ContinuationHead through its CAS. Both halves are asserted from the
    daemon that owns them: byom owns the Continuation, the head revision and
    the Episode states; kovee owns the placement, the invocation fence and
    the binding rows.

    Why a successor EPISODE and not a second attempt of the first one: byom
    records the Manifestation on the Episode at queueing, from the
    PlacementAdmission, and a second `placement_admit` for the same
    allocation is refused (the bridge is already `confirmed`). A resume
    under a DIFFERENT Manifestation is therefore a new activation of the
    same ActivityStream generation — which is exactly what the head CAS is
    for, and what makes the hand-off portable rather than in-process."""
    ev.namespace("continuation-resume")
    tag = "i1cont"
    ctx = governed_setup(ev, tag, {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY},
                         stop_after="activation")
    try:
        byom, kovee, driver = ctx["byom"], ctx["kovee"], ctx["driver"]
        agent, stream = ctx["agent"], ctx["stream"]
        first_episode, first_bound = ctx["episode"], ctx["bound"]
        first_manifestation = byom.row(
            "SELECT manifestation_ref FROM episodes WHERE episode_id = ?",
            first_episode)
        need(first_manifestation == ctx["manifestation"],
             f"the first Episode runs under the admitted attached_harness "
             f"Manifestation: {first_manifestation}")
        need(byom.row("SELECT kind FROM manifestation_revisions"
                      " WHERE manifestation_id = ?", ctx["manifestation"])
             == "attached_harness",
             "byom's own row says what kind the offer proposed")

        # 1. The yielding attempt writes the Continuation: head 0 -> 1.
        first_cont = continuation_write(
            agent, stream, first_episode, first_bound["byom_fence_epoch"],
            0, "summary-i1-yield")
        need(first_cont["head_revision"] == 1,
             f"the head advances to 1: {first_cont}")
        need(first_cont["digest"]["class"] == "local_erasure_safe",
             f"a Continuation is participant-private state, so its digest "
             f"is erasure-safe: {first_cont}")

        # The CAS is a CAS: a second write at the SAME expected revision
        # loses, and a write citing the wrong predecessor is refused.
        stale = agent_refusal(agent, "byom_continuation_write", {
            "activity_stream_ref": stream, "generation": 1,
            "summary_ref": "summary-i1-stale", "unresolved_refs": [],
            "exact_state_refs": [], "source_event_cursor": "cursor-stale",
            "expected_head_revision": 0,
            "classification_ref": "class-participant-private",
            "episode_ref": first_episode,
            "byom_fence_epoch": first_bound["byom_fence_epoch"]})
        need("stale" in stale or "conflict" in stale,
             f"a stale expected_head_revision must lose the CAS: {stale}")
        wrong_prior = agent_refusal(agent, "byom_continuation_write", {
            "activity_stream_ref": stream, "generation": 1,
            "summary_ref": "summary-i1-wrong-prior", "unresolved_refs": [],
            "exact_state_refs": [], "source_event_cursor": "cursor-wrong",
            "prior_continuation_ref": "cont-not-the-head",
            "prior_continuation_digest": first_cont["digest"],
            "expected_head_revision": 1,
            "classification_ref": "class-participant-private",
            "episode_ref": first_episode,
            "byom_fence_epoch": first_bound["byom_fence_epoch"]})
        need(byom.count("SELECT COUNT(*) FROM continuations") == 1,
             "neither refusal wrote a Continuation")
        need(int(byom.row("SELECT continuation_head_revision FROM"
                          " activity_streams WHERE activity_stream_id = ?",
                          stream)) == 1,
             "and neither advanced the head")

        # 2. KOVEE yields the Episode, naming the Continuation a successor
        #    must resume from.
        yielded = driver.ok("episode-yield", {
            "stable_binding_key": first_bound["stable_binding_key"],
            "continuation_ref": first_cont["continuation_id"]})["yielded"]
        need(yielded["continuation_ref"] == first_cont["continuation_id"]
             and yielded["successor_requires_new_binding"] is True,
             f"kovee records the hand-off: {yielded}")
        need(byom.row("SELECT state FROM episodes WHERE episode_id = ?",
                      first_episode) == "yielded",
             "byom's Episode is yielded")
        need(byom.row("SELECT state FROM episode_lease_heads"
                      " WHERE episode_id = ?", first_episode)
             == "lease_yielding",
             "and its lease head is the re-claimable one")

        # 3. The successor activation: a new WakeIntent of the same
        #    participant on the same ActivityStream generation, and byom's
        #    kernel stages again.
        wake2 = agent.one("byom_wake_intent_submit", {
            "activity_stream_ref": stream, "generation": 1,
            "origin": "direct_participant",
            "exact_cause_ref": first_cont["continuation_id"],
            "exact_cause_digest": keyed_of(first_cont["continuation_id"]),
            "purpose_ref": "purpose-explore-i1",
            "stable_wake_key": f"wake-{tag}-2",
            "expires_at": FAR_FUTURE})["result"]["wake_intent_id"]
        requested = agent.one("byom_episode_request", {
            "activity_stream_ref": stream, "generation": 1,
            "wake_intent_ref": wake2,
            "activation_admission_ref": f"adm-{wake2}-r1"})["result"]
        second_episode = requested["episode_id"]
        need(second_episode != first_episode,
             "the successor is its own Episode with its own allocation")

        # 4. KOVEE places the successor under the HOSTED Manifestation,
        #    with a new invocation and therefore a new binding key.
        deployment = kovee.query(
            "SELECT deployment_id, revision, security_profile, status"
            " FROM assistant_deployments WHERE deployment_id = ?",
            (KOVEE_DEPLOYMENT,))
        need(len(deployment) == 1 and deployment[0]["status"] == "active",
             f"the hosted Manifestation is backed by kovee's own active "
             f"deployment record: {deployment}")
        activated = driver.ok("episode-activate", {
            "society_ref": ctx["society"], "recovery_epoch": 0,
            "participant_ref": AGENT,
            "participant_binding_epoch": ctx["binding_epoch"],
            # THE DIFFERENT MANIFESTATION.
            "manifestation_ref": HOSTED_MANIFESTATION,
            "activity_stream_ref": stream, "generation": 1,
            "wake_intent_ref": wake2,
            "kovee_invocation_ref": f"kovee-inv-{tag}-successor",
            "context_manifest_ref": "kovee-ctxman-i1",
            "lease_ttl_seconds": 600,
            "requested": {
                "episode_ref": second_episode, "generation": 1,
                "state": requested["state"],
                "resource_allocation_ref":
                    requested["resource_allocation_id"],
                "resource_allocation_digest":
                    requested["resource_allocation_digest"],
                "parent_budget": requested["parent_budget"]}})
        second_bound = activated["bound"]
        need(second_bound["stable_binding_key"]
             != first_bound["stable_binding_key"],
             "the successor holds a NEW binding (family contract L21/L22)")
        need(second_bound["kovee_invocation_fence"]
             != first_bound["kovee_invocation_fence"]
             or activated["placement"]["placement_id"]
             != ctx["placement"]["placement_id"],
             "under a new Kovee invocation")
        need(byom.row("SELECT state FROM episodes WHERE episode_id = ?",
                      second_episode) == "running",
             "the successor Episode runs")
        second_manifestation = byom.row(
            "SELECT manifestation_ref FROM episodes WHERE episode_id = ?",
            second_episode)
        need(second_manifestation == HOSTED_MANIFESTATION
             and second_manifestation != first_manifestation,
             f"byom's OWN Episode row records the DIFFERENT, hosted "
             f"Manifestation: {second_manifestation} vs "
             f"{first_manifestation}")
        need(byom.row("SELECT selected_manifestation_ref FROM"
                      " placement_admissions WHERE kovee_placement_ref = ?",
                      activated["placement"]["placement_id"])
             == HOSTED_MANIFESTATION,
             "and so does the PlacementAdmission byom committed")
        kovee_placement = kovee.query(
            "SELECT selected_manifestation_ref, host_runtime_binding,"
            " kovee_invocation_ref, kovee_fence_epoch"
            " FROM byom_placement_bindings WHERE placement_id = ?",
            (activated["placement"]["placement_id"],))
        need(len(kovee_placement) == 1
             and kovee_placement[0]["selected_manifestation_ref"]
             == HOSTED_MANIFESTATION,
             f"kovee's own placement row selected it: {kovee_placement}")
        # byom mints no `kovee_deployment` ManifestationRevision — stated,
        # and CHECKED, so the label cannot rot into a false claim.
        need(byom.count("SELECT COUNT(*) FROM manifestation_revisions"
                        " WHERE kind <> 'attached_harness'") == 0,
             "byom still mints only attached_harness ManifestationRevisions")

        # 5. THE RESUME: the successor advances the ONE ContinuationHead,
        #    citing the exact predecessor byom committed for the yielded
        #    attempt — the portable hand-off, under a new fence and a
        #    different Manifestation.
        second_cont = continuation_write(
            agent, stream, second_episode,
            second_bound["byom_fence_epoch"], 1, "summary-i1-resume",
            prior=first_cont)
        need(second_cont["head_revision"] == 2,
             f"the head advances to 2: {second_cont}")
        rows = byom.rows(
            "SELECT continuation_id, head_revision, prior_continuation_ref,"
            " summary_ref FROM continuations WHERE activity_stream_ref = ?"
            " ORDER BY head_revision", (stream,))
        need([r["head_revision"] for r in rows] == [1, 2],
             f"exactly two Continuations, in order: {rows}")
        need(rows[1]["prior_continuation_ref"] == rows[0]["continuation_id"],
             f"the successor's Continuation names its predecessor: {rows}")
        shown = byom.expect_ok("projection", {
            "version": "0.2", "op": "activity_show",
            "activity_stream_ref": stream})["result"]
        need(shown["continuation_head_revision"] == 2,
             f"byom's own read surface reports the head: {shown}")
        # The superseded attempt cannot advance the head any more.
        superseded = agent_refusal(agent, "byom_continuation_write", {
            "activity_stream_ref": stream, "generation": 1,
            "summary_ref": "summary-i1-superseded", "unresolved_refs": [],
            "exact_state_refs": [], "source_event_cursor": "cursor-sup",
            "prior_continuation_ref": second_cont["continuation_id"],
            "prior_continuation_digest": second_cont["digest"],
            "expected_head_revision": 2,
            "classification_ref": "class-participant-private",
            "episode_ref": first_episode,
            "byom_fence_epoch": first_bound["byom_fence_epoch"] + 9})
        need(int(byom.row("SELECT continuation_head_revision FROM"
                          " activity_streams WHERE activity_stream_id = ?",
                          stream)) == 2,
             "a superseded fence cannot advance the head")
        ev.blob("continuations.json", json.dumps(rows, indent=1))
        attribution = cell_attribution(byom, ctx["genesis"],
                                       ctx["sovereign"], ev, "continuation")
        ev.step("plan §8 I1 — CROSS-MANIFESTATION CONTINUATION RESUME: the "
                "first Episode (attached_harness Manifestation) wrote the "
                "participant-owned Continuation and kovee YIELDED it "
                "naming that Continuation; a successor activation of the "
                "same ActivityStream generation was placed by kovee under "
                "a DIFFERENT, HOSTED Manifestation backed by kovee's own "
                "active deployment record, with a new invocation fence and "
                "a new binding key; the successor RESUMED by advancing the "
                "one ContinuationHead 1 -> 2 citing the exact predecessor "
                "ref+digest. Refused along the way: a stale "
                "expected_head_revision, a wrong predecessor, and a write "
                "from a superseded fence. HONEST LIMIT: byom mints "
                "ManifestationRevisions only in `membership_offer` "
                "(`kind: attached_harness`) and has no operation that "
                "admits a `host_kind: kovee_deployment` revision, so the "
                "hosted Manifestation is the one kovee SELECTS at "
                "placement and byom COMMITS on the Episode and the "
                "PlacementAdmission — checked in both stores — not a byom "
                "`host_kind` row",
                first_episode=first_episode,
                first_manifestation=first_manifestation,
                successor_episode=second_episode,
                successor_manifestation=second_manifestation,
                kovee_deployment=deployment[0],
                first_binding_key=first_bound["stable_binding_key"],
                successor_binding_key=second_bound["stable_binding_key"],
                continuation_head=[r["head_revision"] for r in rows],
                predecessor_link=rows[1]["prior_continuation_ref"],
                refusals={"stale_head_revision": stale[:120],
                          "wrong_predecessor": wrong_prior[:120],
                          "superseded_fence": superseded[:120]},
                byom_manifestation_kinds=byom.rows(
                    "SELECT manifestation_id, kind, status FROM"
                    " manifestation_revisions"),
                attributed_event_kinds=sorted({r["kind"]
                                               for r in attribution}))
    finally:
        cleanup_live()


def ambiguous_effect_cell(ev: Evidence) -> None:
    """Plan §8 I1: **one deliberately ambiguous effect walked through
    EOA → disposition**, with the lock ordering and the conservative
    settlement observable.

    The ambiguity is FORCED, not simulated in a fixture: kovee's own
    transport records the send and then reports an uncertain outcome, so
    the request may have been received and billed. What follows is the two
    INDEPENDENT axes of §13.2:

        effect_outcome_admit  (byom runtime, kovee's WORKER channel)
                              verified SOURCE facts only — no decision
                              member exists in the shape at all
        effect_reconcile      (byom governance, the human seat)
                              the LOCAL consequence, under a fresh
                              challenge, against the exact source
                              admission

    Both operations lock the EOA head BEFORE the disposition head, and both
    heads then appear in the downstream dependency closure — asserted from
    byom's own replies, not from the design document."""
    ev.namespace("ambiguous-effect")
    tag = "i1amb"
    ctx = crash_cell_setup("ambiguous-effect", ev, tag)
    try:
        byom, kovee, driver = ctx["byom"], ctx["kovee"], ctx["driver"]
        call_args, act, authorization = armed_broker_state(ctx, tag, "amb")
        key = act["stable_execution_key"]
        base = byom.ledger()

        # 1. FORCE the ambiguity: the send is recorded, the outcome is not.
        outcome = driver.ok("complete", {
            **call_args, "transport": "recording_uncertain",
            "uncertain_reason": "connection reset after the request flushed",
            "authorization": authorization})
        need(outcome["state"] == "ambiguous"
             and outcome["retry_frozen"] is True,
             f"the effect is ambiguous with retry frozen: {outcome}")
        need(driver.durable_sends() == 1,
             f"the request DID leave: {driver.last_sends}")
        effect = driver.ok("effect-show", {"execution_key": key})
        need(effect["usage_reports"] == []
             and byom.count("SELECT COUNT(*) FROM usage_settlements") == 0,
             f"nothing is metered or settled for an unobserved outcome: "
             f"{effect}")

        # 2. byom admits the SOURCE FACT on kovee's own worker channel.
        admitted = driver.ok("effect-admit", {
            "stable_binding_key": ctx["bound"]["stable_binding_key"],
            "execution_key": key,
            "act_intent_ref": authorization["act_intent_ref"],
            "act_intent_digest": authorization["act_intent_digest"]})
        basis = admitted["admission"]
        source = admitted["source"]
        need(source["outcome"] == "ambiguous",
             f"kovee admits what its own rows say: {source}")
        need(source["host_effect_digest"]["class"] == "portable_public"
             and source["host_receipt_digest"]["class"] == "portable_public",
             f"the host-owned digests cross the boundary unkeyed (A8): "
             f"{source}")
        need(basis["outcome"] == "ambiguous" and basis["revision"] == 1,
             f"byom's EffectOutcomeAdmission: {basis}")
        need(basis["lock_order"] == ["effect_outcome_admission_head",
                                     "effect_governance_disposition_head"],
             f"the EOA head is locked BEFORE the disposition head: {basis}")
        closure = basis["dependency_closure"]
        need(closure["effect_outcome_admission_heads"][0]["current_outcome"]
             == "ambiguous",
             f"the source head is in the closure: {closure}")
        need(closure["effect_governance_disposition_heads"] == [],
             f"and the disposition head member is present but empty: "
             f"{closure}")
        need(byom.count("SELECT COUNT(*) FROM governance_decisions"
                        " WHERE kind = 'effect_reconciliation'") == 0,
             "the source path forms NO GovernanceDecision (§13.2 path 1)")
        # An ambiguous source admission carries no result: byom's shape
        # refuses one, so "ambiguous" can never smuggle an outcome.
        need(basis.get("result_ref") in (None, ""),
             f"an ambiguous admission carries no result: {basis}")

        # 3. The DISPOSITION: byom's governance seat, fresh challenge.
        inc = byom.incarnation()

        def reconcile(challenge: str, idem: str) -> dict:
            return byom.call("governance", {
                "version": "0.2", "op": "effect_reconcile",
                "meta": meta(inc, idem),
                "intent_ref": authorization["act_intent_ref"],
                "intent_digest": authorization["act_intent_digest"],
                "stable_execution_key": key,
                "phase": "ambiguous_source",
                "basis_source_admission_ref": basis["admission_id"],
                "basis_source_admission_revision": basis["revision"],
                "basis_source_admission_digest": basis["digest"],
                "local_outcome": "failed",
                "result_use": "unavailable",
                "fresh_challenge_ref": challenge,
                "late_source_policy": "quarantine_and_redecide"})

        disposed = reconcile(f"challenge-{tag}-1", f"{tag}-rec1")
        need(disposed.get("outcome") == "ok", f"reconcile: {disposed}")
        r = disposed["result"]
        need(r["phase"] == "ambiguous_source"
             and r["result_use"] == "unavailable"
             and r["disposition_head_state"] == "active_ambiguous",
             f"the local consequence is recorded, held: {r}")
        need(r["source_head_unchanged"]["current_outcome"] == "ambiguous"
             and r["source_head_unchanged"]["current_admission_ref"]
             == basis["admission_id"],
             f"the disposition never advances the SOURCE head: {r}")
        closure = r["dependency_closure"]
        need(closure["effect_outcome_admission_heads"][0]["current_outcome"]
             == "ambiguous"
             and closure["effect_governance_disposition_heads"][0]["state"]
             == "active_ambiguous",
             f"BOTH heads are now in the downstream closure: {closure}")
        need(closure["lock_order"] == ["effect_outcome_admission_head",
                                       "effect_governance_disposition_head"],
             f"in the same lock order: {closure}")
        decision = r["governance_decision_ref"]
        need(byom.row("SELECT kind FROM governance_decisions"
                      " WHERE decision_id = ?", decision)
             == "effect_reconciliation",
             f"exactly one reconciliation decision: {decision}")
        # A SECOND disposition needs a FRESH challenge: the same one is
        # refused, so a disposition cannot be re-run on stale authority.
        replayed_challenge = reconcile(f"challenge-{tag}-1", f"{tag}-rec2")
        need(replayed_challenge.get("outcome") != "ok",
             f"a second disposition on the SAME challenge must be refused: "
             f"{replayed_challenge}")

        # 4. The CONSERVATIVE settlement, in byom's own ledger: the act's
        #    own reservation is committed (the permit was consumed before
        #    the wire), and NOT ONE unit was charged for an effect nobody
        #    observed — no usage report, no settlement, conservation holds.
        led = byom.ledger()
        act_committed = [x["amount"] for x in
                         byom.reservations(state="committed")
                         if x["holder_kind"] == "act_intent"]
        need(len(act_committed) == 1,
             f"the act's own reservation is committed once: {act_committed}")
        need(led["committed"] == act_committed[0],
             f"and NOTHING else is committed — the ambiguous effect is "
             f"charged for nothing: {led}")
        need(led["conserves"] and led["uncertain"] == 0,
             f"conservation holds: {led}")
        need(byom.count("SELECT COUNT(*) FROM usage_reports") == 0,
             "no usage was reported for the ambiguous effect")
        need(led["remaining"] == base["remaining"],
             f"and the unspent reserve is untouched: {led} vs {base}")
        # The one-shot permit is spent, so the ambiguous effect can never
        # be quietly retried into a second charge.
        retry = driver.problem("complete", {
            **call_args, **RECORDING["args"],
            "authorization": authorization})
        need("spent" in str(retry.get("detail") or ""),
             f"an ambiguous effect is never retried on the same permit: "
             f"{retry}")
        need(driver.durable_sends() == 0,
             f"and that refusal sent nothing: {driver.last_sends}")
        ev.blob("effect-heads.json", json.dumps(
            {"source_admission": basis, "disposition": r,
             "kovee_source_facts": source}, indent=1))
        attribution = cell_attribution(byom, ctx["genesis"],
                                       ctx["sovereign"], ev, "ambiguous")
        ev.step("plan §8 I1 — AMBIGUOUS EFFECT walked through EOA -> "
                "DISPOSITION: kovee's transport recorded the send and "
                "reported an uncertain outcome (external counter = 1), so "
                "the effect is AMBIGUOUS with retry frozen; kovee then "
                "admitted the SOURCE FACTS on byom's worker channel with "
                "portable host effect/receipt digests it derived itself "
                "(no decision member exists in that shape, and byom formed "
                "none), and byom's GOVERNANCE seat recorded the LOCAL "
                "consequence under a FRESH challenge — result_use "
                "unavailable, late_source_policy quarantine_and_redecide. "
                "The EOA head is locked BEFORE the disposition head in "
                "both operations, both heads are in the downstream "
                "dependency closure, the same challenge is refused twice, "
                "and the settlement is CONSERVATIVE: no usage report, no "
                "settlement, nothing committed beyond the act's own "
                "reservation, and a retry refused as spent",
                effect_state="ambiguous", durable_sends=1,
                retry_frozen=True,
                source_admission=basis["admission_id"],
                source_outcome=basis["outcome"],
                lock_order=basis["lock_order"],
                host_effect_ref=source["effect_id"],
                host_receipt_ref=source["host_receipt_ref"],
                host_cursor=source["host_cursor_or_signature_ref"],
                disposition_decision=decision,
                disposition_head=r["disposition_head_state"],
                source_head_after_disposition=r["source_head_unchanged"],
                closure_heads=[
                    closure["effect_outcome_admission_heads"][0][
                        "current_outcome"],
                    closure["effect_governance_disposition_heads"][0][
                        "state"]],
                stale_challenge_refused=(
                    replayed_challenge.get("problem") or {}).get("kind"),
                byom_usage_reports=0, byom_settlements=0,
                ledger=led, act_reservation=act_committed[0],
                retry_refused_as=retry.get("type"),
                attributed_event_kinds=sorted({r["kind"]
                                               for r in attribution}))
    finally:
        cleanup_live()


def production_seal(ev: Evidence) -> dict:
    """The seal behind the daemon's no-network wire, checked on ARTIFACTS
    rather than asserted (R3-I02).

    The gate runs a `koveed/testing` build so the daemon has a recording
    wire to offer. The claim that a PRODUCTION build has none is then
    checked the only way it can be: build `koveed` with no features into a
    separate target directory and look for the recording transport's own
    profile string in the two binaries. It must be in the one this gate
    runs and absent from the production one — the seal is the absence of
    the code, not a flag."""
    # Inside kovee's own target directory, which is always ignored: a second
    # target dir is what makes this a second BUILD rather than an overwrite
    # of the binary the daemons in this run are using.
    seal_dir = _target_dir(KOVEE_ROOT) / "i1-production-seal"
    subprocess.check_call(
        ["cargo", "build", "-q", "-p", "koveed",
         "--manifest-path", str(KOVEE_ROOT / "Cargo.toml")],
        env={**os.environ, "CARGO_TARGET_DIR": str(seal_dir)})
    production = seal_dir / "debug" / "koveed"
    need(production.exists(), f"no production koveed at {production}")
    mark = b"recording-test-double"
    testing_has = mark in Path(koveed_bin()).read_bytes()
    production_has = mark in production.read_bytes()
    need(testing_has,
         "the koveed this gate runs must BE a testing build: its binary "
         "does not carry the recording transport at all, so the completing "
         "worker-socket dispatch could only have gone to a real provider")
    need(not production_has,
         f"a production `cargo build -p koveed` still contains "
         f"{mark.decode()}: the no-network wire is not sealed behind the "
         f"`testing` feature after all ({production})")
    return {"gate_binary": koveed_bin(), "gate_build": "koveed/testing",
            "gate_binary_has_recording_wire": testing_has,
            "production_binary": str(production),
            "production_build": "cargo build -p koveed (no features)",
            "production_binary_has_recording_wire": production_has}


def daemon_egress_cell(ev: Evidence) -> None:
    """Plan §8 I1 / R3-I02: the **completing** model dispatch through
    KOVEED'S OWN worker-socket `model_complete`, over the daemon's own wire.

    The gate used to drive this op only at its refusals and let the
    kovee-linked driver perform every completing call, because
    `koveed::Daemon` built `HttpsTransport` unconditionally: the harness,
    not the daemon, chose the wire — which is the one thing the op is
    supposed to own. `Daemon::with_recording_egress` existed but nothing
    ever called it; deleting the whole feature left kovee's build green.

    It is called now. `koveed/src/main.rs` reads
    `$KOVEE_TESTING_RECORDING_EGRESS` in a `testing` build and hands the
    daemon a `RecordingTransport`; this cell then drives ONE authorized act
    to COMPLETION through the worker socket and holds it to:

      * the stub provider's own reply reaching the worker view — only the
        recording double can produce those bytes, and it opens no socket;
      * kovee's own attempt row recording `recording-test-double`;
      * byom's one-shot permit spent exactly once (one receipt, one
        MandateUse), and refusing the second dispatch on the same path;
      * the DRIVER never running `complete` in this cell at all — the
        evidence directory is the proof, since every driver call writes a
        blob named after its command;
      * and the seal: the production koveed binary contains no such wire.
    """
    ev.namespace("daemon-egress")
    tag = "i1deg"
    seal = production_seal(ev)
    ev.step("R3-I02: the egress the DAEMON offers, and the seal behind it "
            "— this gate runs a `koveed/testing` build whose binary carries "
            "kovee's recording transport, and a production "
            "`cargo build -p koveed` built beside it carries none",
            **seal)
    ctx = governed_setup(ev, tag, {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
    try:
        byom, kovee, driver = ctx["byom"], ctx["kovee"], ctx["driver"]
        call_args, act, authorization = armed_broker_state(ctx, tag, "deg")
        key = act["stable_execution_key"]
        base_ledger = byom.ledger()

        # THE COMPLETING DISPATCH, on koveed's own op. Nothing in this
        # request can name a provider, a host, a header, a credential or a
        # transport: the daemon supplies the wire.
        completed = worker_model_complete(kovee, ctx, RECORDING, "Say OK.",
                                          call_args, authorization)
        need(completed.get("outcome") == "ok",
             f"koveed's own worker-socket model_complete must COMPLETE over "
             f"the daemon's recording wire: {json.dumps(completed)[:900]}")
        r = completed["result"]
        need(r["state"] == "completed", f"the dispatch completed: {r}")
        need(r["usage"]["input_tokens"] == STUB_INPUT_TOKENS
             and r["usage"]["output_tokens"] == STUB_OUTPUT_TOKENS,
             f"the stub provider's own token counts came back through the "
             f"daemon: {r['usage']}")
        need(r.get("provider_ref") == "msg_01i1scripted" and r.get("text")
             == "OK",
             f"and the stub's own reply body: {r}")

        # kovee's own row for the attempt the DAEMON made.
        attempts = kovee.query(
            "SELECT effect_attempt_id, state, transport_profile"
            " FROM model_effect_attempts")
        need(len(attempts) == 1
             and attempts[0]["effect_attempt_id"] == r["effect_attempt_id"]
             and attempts[0]["state"] == "completed"
             and attempts[0]["transport_profile"] == RECORDING["profile"],
             f"exactly one attempt, on the daemon's recorded wire: "
             f"{attempts}")

        # The driver never dispatched anything here: every driver call
        # writes `driver-NN-<command>.json`, and no `complete` is among them.
        prefix = f"{ev.ns}/driver-"
        driver_commands = sorted({
            written[len(prefix):].split("-", 1)[1].removesuffix(".json")
            for written in ev._written if written.startswith(prefix)})
        need("complete" not in driver_commands,
             f"the completing call must be the DAEMON's, not the driver's, "
             f"and the driver ran: {driver_commands}")

        # byom's side of the one-shot permit.
        receipts = byom.count("SELECT COUNT(*) FROM"
                              " execution_consumption_receipts")
        uses = byom.count("SELECT COUNT(*) FROM mandate_uses")
        need(receipts == 1 and uses == 1,
             f"one consumption, one receipt, one MandateUse: "
             f"{receipts}/{uses}")
        need(byom.row("SELECT state FROM act_intents WHERE intent_id = ?",
                      act["intent_id"]) == "consumed",
             "the one-shot act is spent")
        settlements = byom.rows(
            "SELECT charged_quantities, status FROM usage_settlements")
        charged = STUB_INPUT_TOKENS + STUB_OUTPUT_TOKENS
        need(len(settlements) == 1
             and json.loads(settlements[0]["charged_quantities"])
             == [{"dimension": "unit", "unit": "unit", "amount": charged}],
             f"BYOM settled the metered total of the DAEMON's call: "
             f"{settlements}")
        ledger = byom.ledger()
        need(ledger["conserves"], f"conservation holds: {ledger}")

        # And the SPENT permit refuses the second dispatch on the same op.
        again = worker_model_complete(kovee, ctx, RECORDING, "Say OK.",
                                      call_args, authorization,
                                      nonce="-second")
        need(again.get("outcome") != "ok" and "spent" in json.dumps(again),
             f"the one-shot permit refuses a second dispatch on the daemon's "
             f"own path: {json.dumps(again)[:600]}")
        need(byom.count("SELECT COUNT(*) FROM mandate_uses") == 1,
             "and inserts no second MandateUse")
        need(len(kovee.query("SELECT 1 FROM model_effect_attempts")) == 1,
             "and no second attempt")

        attribution = cell_attribution(byom, ctx["genesis"],
                                       ctx["sovereign"], ev, "daemon-egress")
        ev.blob("daemon-completing-dispatch.json", json.dumps(
            {"op": "model_complete", "surface": "koveed worker socket",
             "worker_view": r, "kovee_attempt": attempts[0],
             "byom_receipts": receipts, "byom_mandate_uses": uses,
             "byom_settlement": settlements[0],
             "driver_commands_in_this_cell": driver_commands,
             "seal": seal}, indent=1))
        ev.step("R3-I02 CLOSED: the COMPLETING model dispatch ran inside "
                "koveed's own worker-socket `model_complete` — koveed's "
                "parsing, its worker attempt-binding authentication, its "
                "mutexed store and ITS choice of egress — over the "
                "no-network wire a `koveed/testing` daemon offers. The stub "
                "provider's own reply came back through the op, kovee's "
                "attempt row records recording-test-double, byom spent the "
                "one-shot permit exactly once and settled the metered "
                "total, the second dispatch was REFUSED as spent on the "
                "same path, and the kovee-linked driver ran no `complete` "
                "in this cell at all",
                worker_op="model_complete", state=r["state"],
                usage=r["usage"], provider_ref=r.get("provider_ref"),
                transport_profile=attempts[0]["transport_profile"],
                byom_receipts=receipts, byom_mandate_uses=uses,
                charged=charged, ledger_conserves=ledger["conserves"],
                base_reserved=base_ledger["reserved"],
                second_dispatch_refused_as_spent=True,
                driver_commands_in_this_cell=driver_commands,
                production_seal=seal,
                attributed_event_kinds=sorted({row["kind"]
                                               for row in attribution}))
    finally:
        cleanup_live()


def onboarding_compute_cell(ev: Evidence) -> None:
    """Plan §8 I1: the **one-shot OnboardingCompute path** — a hosted
    candidate's `OnboardingComputeIntent` → `onboarding_compute_permit_
    consume` → `OnboardingComputeReceipt`, with completion as EVIDENCE
    ONLY. This is what makes C2's integration acceptance genuinely covered
    at I1 (onboarding-compute here, dispatch at I2).

    Two honest limits, both checked rather than asserted away:

      * kovee has NO onboarding code (`grep -rn onboarding kovee/crates`
        finds nothing), so no kovee subsystem owns this call. The consume,
        the claim and the completion are sent by kovee's own byom client
        from the kovee-linked driver, as the hosted candidate's runtime,
        and the DECISION to call is the scenario's.
      * byom's `onboarding_compute_permit_consume` shape demands
        `local_erasure_safe` (byom-keyed) digests for three KOVEE-owned
        objects — the provider context manifest, the disclosure manifest
        and the model profile — which no kovee derivation can produce.
        That is the same A8 direction R3-L01 closed for
        `execution_permit_consume`, still open here; the three digests
        therefore come from this scenario and the evidence says so. Every
        value the receipt is CHECKED against is byom's own."""
    ev.namespace("onboarding-compute")
    tag = "i1onb"
    candidate = "part-cand-hosted"
    byom = ByomDaemon(tag, {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
    kovee = Koveed(tag, byom, {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
    driver = Driver(kovee, byom, ev, {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
    try:
        booted = bootstrap_society(byom, tag, ev)
        society = booted["society"]
        enabled = kovee_bind_governance(kovee, byom, society, driver, ev)
        install_host_binding(driver, byom, enabled, ev)
        inc = byom.incarnation()
        subject = digest(0xB7)

        # The Society OFFERS membership to a HOSTED candidate and then FUNDS
        # the bounded onboarding activation with its one compute use.
        offered = byom.expect_ok("governance", {
            "version": "0.2", "op": "membership_offer",
            "meta": meta(inc, f"{tag}-offer"),
            "participant_ref": candidate,
            "proposed_standing_ref": "standing-proposal-hosted",
            "subject_digest": subject,
            "offered_by_decision_ref": f"dec-society-{society}",
            "expires_at": FAR_FUTURE})["result"]
        offer_id = offered["offer_id"]
        onboarding = f"onb-{offer_id}"
        intent = f"oci-{onboarding}"
        funded = byom.expect_ok("governance", {
            "version": "0.2", "op": "onboarding_offer",
            "meta": meta(inc, f"{tag}-fund"),
            "membership_offer_ref": offer_id,
            "candidate_participant_ref": candidate,
            # The HOSTED Manifestation: byom binds its ref and digest here
            # without minting a ManifestationRevision at all.
            "proposed_manifestation_ref": HOSTED_MANIFESTATION,
            "proposed_manifestation_digest": digest(0xB8),
            "exact_context_ref": "ctx-onb-minimal",
            "exact_context_digest": digest(0xB9),
            "resource_reservation_ref": "resv-onb-i1",
            "onboarding_compute_intent_ref": intent,
            "expires_at": FAR_FUTURE,
            "adopted_by_decision_ref": f"dec-offer-{offer_id}"})["result"]
        need(funded["max_episodes"] == 1
             and funded["general_effect_and_child_authority"] == "none"
             and funded["membership_offer_state"] == "onboarding",
             f"the offer is bounded by the RECORD, not by a request member: "
             f"{funded}")
        need(funded["allowed_operations"] == [
                 "membership_refuse", "membership_accept",
                 "candidate_self_policy_propose"],
             f"§7.4 verbatim: the candidate channel may only refuse, "
             f"accept, or return proposed policies: {funded}")
        need(byom.row("SELECT state FROM onboarding_compute_intents"
                      " WHERE compute_intent_id = ?", intent)
             == "authorized",
             "the one-shot compute intent is authorized by the funding")
        intent_digest = json.loads(byom.row(
            "SELECT digest FROM onboarding_compute_intents"
            " WHERE compute_intent_id = ?", intent))

        # kovee's own model profile, and a REAL kovee disclosure manifest id
        # is not available on this path (there is no kovee onboarding
        # broker), so the three peer-owned digests are the scenario's —
        # named as such in the evidence.
        consume_args = {
            "compute_intent_ref": intent,
            "compute_intent_digest": intent_digest,
            "stable_compute_key": f"occ-{intent}",
            "meta_key": f"occ-{intent}-c1",
            "onboarding_fence_epoch": 1,
            "expected_revision": funded["revision"],
            "kovee_invocation_ref": f"kovee-inv-{tag}-onb",
            "provider_context_manifest_ref": "kovee-pcm-onb-i1",
            "provider_context_manifest_digest": digest(0xC3),
            "disclosure_manifest_ref": "kovee-disclosure-onb-i1",
            "disclosure_manifest_digest": digest(0xC4),
            "model_profile_ref": RECORDING["model_profile_ref"],
            "model_profile_digest": digest(0xC5)}
        receipt = driver.ok("onboarding-consume", consume_args)["receipt"]
        need(receipt["max_uses"] == 1,
             f"§7.4: at most ONE compute use per offer: {receipt}")
        need(all(receipt["grants"][g] == "none" for g in
                 ("tools", "network", "workspace", "children",
                  "reusable_participant_authority")),
             f"the receipt grants NOTHING beyond the one compute: {receipt}")
        receipt_id = receipt["receipt_id"]
        need(byom.row("SELECT state FROM onboarding_compute_intents"
                      " WHERE compute_intent_id = ?", intent) == "consumed",
             "the one-shot authority is spent")

        # A SECOND compute use with a CHANGED final manifest is refused.
        second = driver.problem("onboarding-consume", {
            **consume_args,
            "meta_key": f"occ-{intent}-c2",
            "expected_revision": funded["revision"] + 1,
            "provider_context_manifest_digest": digest(0x0C)})
        need("ONE compute use" in json.dumps(second),
             f"a second compute use must be refused: {second}")
        need(byom.count("SELECT COUNT(*) FROM onboarding_compute_receipts")
             == 1, "and mint no second receipt")
        # The EXACT retry recovers the stored receipt.
        retried = driver.ok("onboarding-consume", {
            **consume_args,
            "meta_key": f"occ-{intent}-c3",
            "expected_revision": funded["revision"] + 1})["receipt"]
        need(retried["replayed"] is True
             and retried["receipt_id"] == receipt_id,
             f"the exact retry replays the stored receipt: {retried}")
        need(byom.count("SELECT COUNT(*) FROM onboarding_compute_receipts")
             == 1, "still exactly one receipt")

        # The ONE onboarding Episode, claimed by the HOSTED runtime, citing
        # the receipt; a second is refused.
        receipt_digest = json.loads(byom.row(
            "SELECT digest FROM onboarding_compute_receipts"
            " WHERE receipt_id = ?", receipt_id))
        claim_args = {
            "onboarding_ref": onboarding,
            "candidate_participant_ref": candidate,
            "proposed_manifestation_ref": HOSTED_MANIFESTATION,
            "proposed_manifestation_digest": digest(0xB8),
            "onboarding_fence_epoch": 1,
            "holder_runtime_binding": f"kovee-runtime-{KOVEE_DEPLOYMENT}",
            "stable_claim_key": f"onbclaim-{tag}-1",
            "compute_receipt_ref": receipt_id,
            "compute_receipt_digest": receipt_digest}
        claimed = driver.ok("onboarding-claim", claim_args)["claim"]
        episode = claimed["onboarding_episode_id"]
        need(claimed["max_episodes"] == 1
             and claimed["acceptance_effect"] == "none",
             f"the onboarding Episode grants no acceptance: {claimed}")
        need(claimed["allowed_output_operations"] == [
                 "refuse", "membership_accept",
                 "candidate_self_policy_propose"],
             f"§7.4 verbatim, on the output side too: {claimed}")
        second_claim = driver.problem("onboarding-claim", {
            **claim_args, "stable_claim_key": f"onbclaim-{tag}-2"})
        need("max_episodes" in json.dumps(second_claim),
             f"a second onboarding Episode is refused: {second_claim}")

        # COMPLETION IS EVIDENCE ONLY.
        completed = driver.ok("onboarding-complete", {
            "onboarding_ref": onboarding,
            "onboarding_episode_ref": episode,
            "onboarding_fence_epoch": 1,
            "expected_revision": 1,
            "outcome": "completed",
            "output_refs": ["candidate-output-i1"],
            "evidence_refs": ["candidate-evidence-i1"]})["completion"]
        need(completed["completion_is_evidence_only"] is True,
             f"the reply says so: {completed}")
        need(completed["acceptance"] == {
                 "membership_accepted": False,
                 "membership_acceptance_ref": None,
                 "standing_created": False,
                 "participant_authority_granted": False},
             f"and names everything that did NOT happen: {completed}")
        need(byom.row("SELECT state FROM membership_offers"
                      " WHERE offer_id = ?", offer_id) == "onboarding",
             "runtime output is never membership assent")
        need(byom.row("SELECT acceptance_id FROM membership_offers"
                      " WHERE offer_id = ?", offer_id) is None,
             "no MembershipAcceptance follows a completed compute")
        need(byom.count("SELECT COUNT(*) FROM standing_revisions"
                        f" WHERE participant_ref = '{candidate}'") == 0,
             "no Standing follows a completed compute")
        need(byom.row("SELECT state FROM participants"
                      " WHERE participant_id = ?", candidate) == "proposed",
             "the candidate is still only proposed")

        # Only the CANDIDATE's own act accepts — over its own channel.
        #
        # Not through byom-mcp here, and the reason is worth recording:
        # `bridge.rs` derives `expected_revision: 1` for
        # `membership_accept` (the offer's MINTED revision), while an
        # onboarding-FUNDED offer sits at revision 2, so the tool binding
        # answers `stale_revision` on this path. The candidate's own
        # channel accepts it directly, with the revision byomd committed.
        offer_revision = int(byom.row(
            "SELECT revision FROM membership_offers WHERE offer_id = ?",
            offer_id))
        accepted = channel_socket_call(
            byom, byom.token_file(f"candidate-{offer_id}.token"),
            {"version": "0.2", "op": "membership_accept",
             "meta": meta(byom.incarnation(), f"{tag}-accept",
                          offer_revision),
             "offer_ref": offer_id, "subject_digest": subject})
        need(accepted.get("outcome") == "ok"
             and accepted["result"]["offer_state"] == "accepted",
             f"the candidate's own acceptance: {accepted}")
        need(byom.row("SELECT state FROM membership_offers"
                      " WHERE offer_id = ?", offer_id) == "accepted",
             "only the candidate's own act accepts")
        attribution = cell_attribution(byom, booted["genesis"],
                                       sovereign_id(byom, society), ev,
                                       "onboarding")
        ev.step("plan §8 I1 — the ONE-SHOT OnboardingCompute path: the "
                "Society funded a bounded OnboardingActivationOffer for a "
                "HOSTED candidate (max_episodes 1, authority `none`, three "
                "allowed candidate operations, all record constants); "
                "kovee's client consumed the one-shot compute permit on "
                "byom's BROKER channel and got an OnboardingComputeReceipt "
                "with max_uses 1 and every grant `none`; a CHANGED second "
                "use was refused and the exact retry replayed the stored "
                "receipt; the ONE onboarding Episode was claimed by the "
                "hosted runtime citing that receipt and a second claim was "
                "refused; and COMPLETION IS EVIDENCE ONLY — the offer is "
                "still `onboarding`, there is no acceptance, no Standing "
                "and no participant authority, and only the CANDIDATE's "
                "own act on its own channel accepted. HONEST LIMITS: kovee "
                "ships no onboarding code, so the caller is kovee's client "
                "and the trigger is the scenario's; and byom's shape still "
                "demands byom-keyed digests for three kovee-owned objects "
                "(the A8 direction R3-L01 closed for "
                "execution_permit_consume), so those three values come "
                "from this scenario",
                candidate=candidate, offer=offer_id,
                onboarding=onboarding, compute_intent=intent,
                receipt=receipt_id, receipt_max_uses=1,
                grants=receipt["grants"],
                second_use_refused=True, exact_retry_replayed=True,
                onboarding_episode=episode,
                second_episode_refused=True,
                completion_is_evidence_only=True,
                acceptance=completed["acceptance"],
                offer_state_after_completion="onboarding",
                accepted_by="the candidate's own membership_accept on its "
                            "own channel (byom-mcp pins expected_revision 1 "
                            "for that op, so the tool binding cannot accept "
                            "an onboarding-FUNDED offer at revision 2)",
                scenario_supplied_digests=[
                    "provider_context_manifest_digest",
                    "disclosure_manifest_digest", "model_profile_digest"],
                attributed_event_kinds=sorted({r["kind"]
                                               for r in attribution}))
    finally:
        cleanup_live()


# ------------------------------------------------------------ scripted ----

RECORDING = {
    "model_profile_ref": "mp-anthropic-realm-personal",
    "provider_binding_ref": "mpb-anthropic-realm-personal",
    "profile": "recording-test-double",
    "expect_send_count": 1,
    "args": {"transport": "recording", "reply_body": STUB_REPLY},
}


# The plan-§8 I1 item list and, per item, the PROOFS this gate accepts for
# it: which cell of which mode may certify it, the step that cell has to
# have printed, the artifacts it has to have written, and what — if
# anything — is still standing in (R3-I01).
#
# The old map validated only that its caller supplied a non-default string
# per cell. A run of no-op cells satisfied it, so "coverage" certified
# nothing but the caller's own prose. Every proof now names:
#
#   step:      a substring of a step title THIS run printed in that cell,
#              read back from the Evidence object's own record of what it
#              printed — not from a string the caller composed;
#   artifacts: files THIS run wrote under `evidence/<test-id>/<cell>/`,
#              each of which must exist, be non-empty and be one this
#              process wrote (`Evidence` tracks every path it writes, so a
#              file left behind by an earlier run is not evidence);
#   simulated: exactly what is standing in. A simulated proof is REPORTED
#              AS SIMULATED — in the coverage blob, in the step detail and
#              on stdout — and is never counted as covered.
#
# An item with two proofs (the attached execution paths) is covered by
# whichever ran; an unsimulated proof supersedes a simulated one, so the
# real CLI session upgrades the deterministic stand-in when `--all-checks`
# runs both.
WIRE_ONLY = (
    "the WIRE, and only the wire. The provider is kovee's own "
    "`RecordingTransport`, which opens no socket and stamps "
    "`recording-test-double` on the effect. A real provider call is "
    "`--real-model` (it spends money).")

PLAN_8_I1_ITEMS = [
    {"item": "greenfield binding saga",
     "proofs": {"governed-loop": {
         "step": "governance_enable — the D10 GREENFIELD saga",
         "artifacts": ["driver-01-host-binding.json",
                       "byom-timeline.json"]}}},
    {"item": "AttentionContract notification (never a wake)",
     "proofs": {"governed-loop": {
         "step": "attention_notice_record",
         "artifacts": ["driver-02-attention-notice.json"],
         "simulated":
             "the TRIGGER. kovee's `kovee-attention` crate is a two-line "
             "stub, so no AttentionContract subsystem DECIDES to notify and "
             "kovee has no `Workload::Attention` channel class; this "
             "scenario decides. The notice itself is real — kovee's own "
             "byom client verifies the event is in koveed's ledger, derives "
             "the source digest and sends it on byomd's narrow attention "
             "channel — and byom's no-effect arm is asserted from byom's "
             "own rows."}}},
    {"item": "the complete activation pipeline in A8 order",
     "proofs": {"governed-loop": {
         "step": "episode_request (participant)",
         "artifacts": ["byom-timeline.json"]}}},
    {"item": "hosted episode",
     "proofs": {"governed-loop": {
         "step": "PlacementBinding (the ONE activation record Kovee owns)",
         "artifacts": ["driver-06-episode-activate.json"]}}},
    {"item": "cross-Manifestation Continuation resume",
     "proofs": {"continuation-resume": {
         "step": "CROSS-MANIFESTATION CONTINUATION RESUME",
         "artifacts": ["continuations.json",
                       "driver-07-episode-yield.json"]}}},
    {"item": "a distinct hosted Manifestation",
     "proofs": {"continuation-resume": {
         "step": "CROSS-MANIFESTATION CONTINUATION RESUME",
         "artifacts": ["driver-08-episode-activate.json"],
         "simulated":
             "the byom ManifestationRevision. byom mints revisions only "
             "inside `membership_offer`, which fixes `kind: "
             "attached_harness`; no byom operation admits a `host_kind: "
             "kovee_deployment` revision and `placement_admit` does not "
             "resolve `selected_manifestation_ref` against that table. The "
             "cell CHECKS exactly that — byom holds zero "
             "non-attached_harness revisions — so the distinct hosted "
             "Manifestation is the ref kovee SELECTS at placement from its "
             "own active deployment row and byom COMMITS on the Episode and "
             "the PlacementAdmission, asserted in both stores, and NOT a "
             "byom `host_kind` row."}}},
    {"item": "ambiguous effect: EOA -> disposition, lock order, "
             "conservative settlement",
     "proofs": {"ambiguous-effect": {
         "step": "AMBIGUOUS EFFECT walked through EOA",
         "artifacts": ["effect-heads.json", "driver-09-complete.json"]}}},
    {"item": "execution path 1/3: hosted invocation through the disclosed "
             "metered broker",
     "proofs": {"governed-loop": {
         "step": "kovee broker PROCEEDS with the permit",
         "artifacts": ["kovee-broker-chain.json"],
         "simulated": WIRE_ONLY}}},
    {"item": "the completing model dispatch inside koveed's OWN "
             "worker-socket `model_complete`",
     "proofs": {"daemon-egress": {
         "step": "R3-I02 CLOSED",
         "artifacts": ["daemon-completing-dispatch.json"],
         "simulated":
             "the WIRE, and only the wire — chosen by the DAEMON, not by "
             "this harness. koveed is built `--features testing`, the only "
             "build in which it has a `RecordingTransport` to offer; the "
             "production binary is built beside it in the same cell and "
             "checked to contain no such wire at all."}}},
    {"item": "execution path 2/3: attached Claude Code",
     "proofs": {
         "attached-claude": {
             "step": "ATTACHED claude",
             "artifacts": ["attached-steps.json", "participant-tools.json"],
             "simulated":
                 "the CLI SESSION. Every agent step goes through the real "
                 "byom-mcp participant surface with the exact tool "
                 "allowlist and launch argv `--harness claude` uses, and no "
                 "claude CLI is invoked, so the cell is deterministic. The "
                 "real session is `--harness claude` under "
                 "I1_REAL_HARNESS=1, which `--all-checks` runs and reports "
                 "— and which supersedes this proof when it passes."},
         "harness-claude": {
             "step": "real claude sessions drove the agent's own steps",
             "artifacts": ["session-01-byom_mandate_prepare.txt",
                           "session-01-byom_mandate_prepare.byom-wire.jsonl"],
             "simulated": WIRE_ONLY}}},
    {"item": "execution path 3/3: attached Codex",
     "proofs": {
         "attached-codex": {
             "step": "ATTACHED codex",
             "artifacts": ["attached-steps.json", "participant-tools.json"],
             "simulated":
                 "the CLI SESSION. Every agent step goes through the real "
                 "byom-mcp participant surface with the exact tool "
                 "allowlist and launch argv `--harness codex` uses, and no "
                 "codex CLI is invoked, so the cell is deterministic. The "
                 "real session is `--harness codex` under "
                 "I1_REAL_HARNESS=1, which `--all-checks` runs and reports "
                 "— and which supersedes this proof when it passes."},
         "harness-codex": {
             "step": "real codex sessions drove the agent's own steps",
             "artifacts": ["session-01-byom_mandate_prepare.txt",
                           "session-01-byom_mandate_prepare.byom-wire.jsonl"],
             "simulated": WIRE_ONLY}}},
    {"item": "the one-shot OnboardingCompute path",
     "proofs": {"onboarding-compute": {
         "step": "ONE-SHOT OnboardingCompute path",
         "artifacts": ["driver-07-onboarding-complete.json"],
         "simulated":
             "the CALLER and three digests. kovee ships no onboarding code "
             "at all, so the consume/claim/complete are sent by kovee's own "
             "byom client acting as the hosted candidate's runtime and the "
             "decision to call is this scenario's; and byom's "
             "`onboarding_compute_permit_consume` still demands byom-keyed "
             "(`local_erasure_safe`) digests for three KOVEE-owned objects "
             "— the A8 direction R3-L01 closed for "
             "`execution_permit_consume` — so those three values come from "
             "this scenario. Every value the receipt is CHECKED against is "
             "byom's own."}}},
    {"item": "data boundary: synthetic data, provider claims bound by "
             "ref+digest, honest developer/confined labels",
     "proofs": {"governed-loop": {
         "step": "assurance profile, labeled honestly",
         "artifacts": ["byom-attribution.json"]}}},
]

# Where a mode's coverage statement lands, so `--all-checks` can hold the
# WHOLE gate to the item list rather than any one mode.
COVERAGE_BLOB = "plan-8-i1-coverage.json"


def plan_coverage(ev: Evidence, covered: dict) -> list:
    """This MODE's coverage statement, checked against its OWN evidence.

    `covered` maps the cells this mode ran to how it ran them. For each
    such cell the item's proof must hold: the step it claims must be one
    THIS run printed in that cell, and every artifact it names must be a
    file THIS run wrote and left non-empty. A cell that did nothing cannot
    be certified — which is exactly what the old string map allowed.

    A cell this mode did not run is reported `not covered by this mode`;
    `mode_all_checks` is where the union has to cover every item. A mode
    can therefore no longer certify what it did not do, and the gate can no
    longer pass with an item nobody did.

    Anything standing in is reported as `simulated` with the reason. That
    is an honest state and does not fail the mode; an item with a cell but
    no step or no artifact IS a failure."""
    printed = {}
    for cell, title in ev.titles:
        printed.setdefault(cell, []).append(title)
    rows, missing = [], []
    for spec in PLAN_8_I1_ITEMS:
        item = spec["item"]
        ran = [c for c in spec["proofs"] if c in covered]
        row = {"plan_8_I1_item": item,
               "cells_that_could_prove_it": sorted(spec["proofs"]),
               "status": "not covered by this mode", "proofs": []}
        for cell in ran:
            proof = spec["proofs"][cell]
            marker = proof.get("step")
            if not any(marker in title for title in printed.get(cell, [])):
                # This mode ran the cell but not the step that proves this
                # item — `--verify-trails`, for instance, runs the whole arc
                # without printing the honesty labels. That is "not covered
                # HERE", not a broken claim; `mode_all_checks` is where the
                # union has to cover every item, so nothing can hide in the
                # gap between two modes.
                continue
            artifacts, ok = [], True
            for name in proof.get("artifacts", []):
                key = f"{cell}/{name}"
                path = ev.dir / key
                if key not in ev._written:
                    missing.append(
                        f"{item!r}: {key} was not written by this run — a "
                        f"leftover file is not evidence")
                    ok = False
                    continue
                if not path.exists() or path.stat().st_size == 0:
                    missing.append(f"{item!r}: {key} is missing or empty")
                    ok = False
                    continue
                artifacts.append({"path": key, "bytes": path.stat().st_size,
                                  "written_at_step": ev._written[key]})
            if not ok:
                continue
            row["proofs"].append({
                "cell": cell, "how": covered[cell], "proved_by_step": marker,
                "proved_by_artifacts": artifacts,
                "simulated": proof.get("simulated")})
        real = [p for p in row["proofs"] if not p["simulated"]]
        if real:
            row["status"] = "exercised"
        elif row["proofs"]:
            row["status"] = "SIMULATED"
            row["simulated"] = row["proofs"][0]["simulated"]
        rows.append(row)
    need(not missing,
         "plan §8 I1 coverage is not backed by this run's own evidence:\n  "
         + "\n  ".join(missing))
    ev.namespace(None)
    ev.blob(COVERAGE_BLOB, json.dumps(rows, indent=1))
    return rows


def merge_coverage(per_mode: list) -> list:
    """The WHOLE gate's coverage: every mode's statement, unioned per item,
    with an unsimulated proof superseding a simulated one."""
    merged = {spec["item"]: {"plan_8_I1_item": spec["item"],
                             "cells_that_could_prove_it":
                                 sorted(spec["proofs"]),
                             "status": "NOT EXERCISED", "proofs": []}
              for spec in PLAN_8_I1_ITEMS}
    for rows in per_mode:
        for row in rows:
            target = merged.get(row["plan_8_I1_item"])
            if target is None:
                continue
            target["proofs"] += row.get("proofs", [])
    for row in merged.values():
        real = [p for p in row["proofs"] if not p["simulated"]]
        if real:
            row["status"] = "exercised"
        elif row["proofs"]:
            row["status"] = "SIMULATED"
            row["simulated"] = row["proofs"][0]["simulated"]
    return list(merged.values())


def print_coverage(rows: list, heading: str):
    """The coverage statement in the runner's OWN output, with every
    simulation named where a reader cannot miss it (R3-I01)."""
    simulated = [r for r in rows if r["status"] == "SIMULATED"]
    absent = [r for r in rows if r["status"].startswith("NOT")
              or r["status"].startswith("not")]
    print(f"\n{heading}")
    for row in rows:
        cells = ",".join(sorted({p["cell"] for p in row["proofs"]})) or "-"
        print(f"  [{row['status']:<22}] {row['plan_8_I1_item']}  "
              f"({cells})")
    print(f"  {len(rows) - len(simulated) - len(absent)} of {len(rows)} "
          f"plan-§8 I1 items are exercised with NOTHING standing in; "
          f"{len(simulated)} are SIMULATED and {len(absent)} are not "
          f"covered here.")
    for row in simulated:
        print(f"    - SIMULATED: {row['plan_8_I1_item']}\n"
              f"      what stands in: {row['simulated']}")

def mode_scripted() -> int:
    ev = Evidence("i1-flow-scripted")
    print("i1-flow-scripted: the governed loop across both live daemons, "
          "with a STUB provider (no network) so the gate is deterministic")
    ctx = None
    try:
        pinned = assert_pinned(ev)
        ev.step("the revisions this gate is running, ASSERTED: the driver "
                "reports the kovee commit its own build.rs read out of the "
                "tree it links (a mismatch fails the BUILD), every daemon "
                "and CLI binary was rebuilt in this run, and each one is "
                "newer than every source file cargo says it was compiled "
                "from",
                byom=pinned["byom"], kovee=pinned["kovee"],
                driver_built_against=pinned["driver_built_against"],
                explicit_pins=pinned["explicit_pins"])
        ev.namespace("governed-loop")
        ctx = scripted_flow(ev, "i1s", RECORDING,
                            {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY},
                            "Say OK.")
        need(ctx["usage"]["input_tokens"] == STUB_INPUT_TOKENS
             and ctx["usage"]["output_tokens"] == STUB_OUTPUT_TOKENS,
             f"the stub's own token counts reached byom: {ctx['usage']}")
        per_source_trails(ctx, ev, "scripted")
        honesty_labels(ev, RECORDING,
                       "the scripted gate substitutes ONLY the wire: "
                       "kovee's own RecordingTransport, which opens no "
                       "socket and stamps recording-test-double on the "
                       "effect")
        cleanup_live()
        # The plan-§8 I1 items the old gate excluded, each on its own live
        # pair of daemons (R3-I01).
        continuation_resume_cell(ev)
        ambiguous_effect_cell(ev)
        daemon_egress_cell(ev)
        onboarding_compute_cell(ev)
        oracle_self_test(ev)
        rows = plan_coverage(ev, {
            "governed-loop": "exercised live in this run",
            "continuation-resume": "exercised live in this run",
            "ambiguous-effect": "exercised live in this run",
            "daemon-egress": "exercised live in this run",
            "onboarding-compute": "exercised live in this run"})
        ev.step("plan §8 I1 coverage, item by item, CHECKED AGAINST THIS "
                "RUN'S OWN EVIDENCE: every claim above names a step this "
                "run printed in that cell and artifacts this run wrote and "
                "left non-empty, so a no-op cell cannot certify anything. "
                "The two ATTACHED execution paths are not in this mode at "
                "all and are reported as such; they are covered by "
                "`--attached-path <which>` and, for real, by "
                "`--harness <which>`. Everything still standing in is "
                "named as SIMULATED with its reason and is NOT counted as "
                "covered",
                coverage=rows)
        print_coverage(rows, "i1-flow-scripted: plan §8 I1 coverage "
                             "(this mode):")
        print(f"i1-flow-scripted: PASS ({ev.n} steps; evidence "
              f"{ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()
        cleanup_live()


def mode_verify_trails() -> int:
    """i1-trails: the per-source attribution focus.

    The same live arc, with the actor map checked EXHAUSTIVELY over every
    byom event of every cell — and the map's own negative run last: a
    synthetic event of an unmapped kind must FAIL the check, so a record
    this gate has never seen cannot pass unnoticed."""
    ev = Evidence("i1-trails")
    print("i1-trails: per-source attribution — byom's events over byom's "
          "records, kovee's over kovee's, with an EXHAUSTIVE actor map")
    ctx = None
    try:
        pinned = assert_pinned(ev)
        ev.step("the revisions this run is checking, ASSERTED (R3-I02)",
                byom=pinned["byom"], kovee=pinned["kovee"],
                driver_built_against=pinned["driver_built_against"])
        ev.namespace("governed-loop")
        ctx = scripted_flow(ev, "i1t", RECORDING,
                            {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY},
                            "Say OK.")
        per_source_trails(ctx, ev, "trails")
        sov = ctx["sovereign"]
        cleanup_live()
        continuation_resume_cell(ev)
        ambiguous_effect_cell(ev)
        onboarding_compute_cell(ev)
        ev.namespace(None)
        # The map's own negative: an unmapped kind must FAIL.
        try:
            verify_byom_attribution([{"kind": "not.a.mapped.kind",
                                      "actor_ref": GOV_ACTOR}], sov)
        except Fail as refusal:
            ev.step("the actor map is exhaustive BY CONSTRUCTION: a "
                    "synthetic event of an unmapped kind FAILS the check "
                    "instead of being skipped — so a new record kind "
                    "cannot enter either daemon's trail unattributed",
                    refusal=str(refusal))
        else:
            raise Fail("an unmapped event kind must fail --verify-trails")
        # And an event with the WRONG author fails too.
        try:
            verify_byom_attribution([{"kind": "resource-allocation.reserved",
                                      "actor_ref": f"participant:{AGENT}"}],
                                    sov)
        except Fail as refusal:
            ev.step("and an event authored by the WRONG actor fails: the "
                    "kernel's own stage-3 allocation attributed to the "
                    "agent is refused, which is the property the whole map "
                    "exists to hold",
                    refusal=str(refusal))
        else:
            raise Fail("a mis-attributed kernel record must fail")
        rows = plan_coverage(ev, {
            "governed-loop": "exercised live in this run",
            "continuation-resume": "exercised live in this run",
            "ambiguous-effect": "exercised live in this run",
            "onboarding-compute": "exercised live in this run"})
        print_coverage(rows, "i1-trails: plan §8 I1 coverage (this mode):")
        print(f"i1-trails: PASS ({ev.n} steps; evidence "
              f"{ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()
        cleanup_live()


# ---------------------------------------------------------- real model ----

# The two providers kovee ships a binding for. `env` is the variable the
# binding's `credential_secret_ref` names (`env:NAME`) — the DAEMON's own
# environment, which the broker is the only reader of.
PROVIDERS = [
    {"env": "ANTHROPIC_API_KEY", "file": "claude", "kind": "anthropic",
     "model_profile_ref": "mp-anthropic-realm-personal",
     "provider_binding_ref": "mpb-anthropic-realm-personal",
     "profile": "https-tls13", "args": {"transport": "https"}},
    {"env": "OPENAI_API_KEY", "file": "openai", "kind": "openai",
     "model_profile_ref": "mp-openai-realm-personal",
     "provider_binding_ref": "mpb-openai-realm-personal",
     "profile": "https-tls13", "args": {"transport": "https"}},
]


def api_key_for(provider: dict) -> tuple[str, str] | None:
    """One provider credential, in the plan's precedence: an already-set
    environment variable first, else the key file under
    `$KOVEE_API_KEY_DIR` (default `~/.api`).

    The VALUE is returned in memory only. It goes nowhere but the daemon's
    and the driver's environment: never a log line, never an evidence
    blob, never a process argument. Only the SOURCE is ever recorded."""
    value = os.environ.get(provider["env"], "").strip()
    if value:
        return value, f"env:{provider['env']}"
    directory = Path(os.environ.get("KOVEE_API_KEY_DIR",
                                    str(Path.home() / ".api")))
    path = directory / provider["file"]
    try:
        value = path.read_text(encoding="utf-8").strip()
    except OSError:
        return None
    return (value, f"file:{path}") if value else None


def scan_for_secret(paths: list, secret: str) -> list:
    """Every place a key could have leaked: both daemons' database files
    (including their WAL and journal sidecars), byomd's channel directory,
    and every evidence blob this run wrote. kovee's own k2_broker asserts
    the credential reaches no worker-visible record; this is the same
    assertion over the SCENARIO's whole footprint."""
    needle = secret.encode()
    hits = []
    for root in paths:
        root = Path(root)
        if not root.exists():
            continue
        files = [root] if root.is_file() else sorted(root.rglob("*"))
        for f in files:
            if not f.is_file():
                continue
            try:
                if needle in f.read_bytes():
                    hits.append(str(f))
            except OSError:
                continue
    return hits


def mode_real_model() -> int:
    ev = Evidence("i1-real-model")
    present = [(p, api_key_for(p)) for p in PROVIDERS]
    available = [(p, k) for p, k in present if k is not None]
    if os.environ.get("I1_REAL_MODEL") != "1":
        print("i1-real-model: SKIP (env-gated; set I1_REAL_MODEL=1 to make "
              "REAL provider calls through the broker). Keys that WOULD be "
              "used: "
              + (", ".join(f"{p['env']} <- {k[1]}" for p, k in available)
                 or "none found (env or $KOVEE_API_KEY_DIR, default "
                    "~/.api/{claude,openai})"))
        ev.close()
        return 2
    if not available:
        print("i1-real-model: SKIP — I1_REAL_MODEL=1 but no provider key is "
              "present. Set ANTHROPIC_API_KEY / OPENAI_API_KEY, or place "
              "the key in $KOVEE_API_KEY_DIR (default ~/.api) as "
              "`claude` / `openai`.")
        ev.close()
        return 2
    print(f"i1-real-model: the SAME loop with a REAL provider call through "
          f"the broker, for {len(available)} provider(s): "
          f"{', '.join(p['kind'] for p, _ in available)}")
    try:
        # EVERY mode pins, and this is the one that spends money on the
        # answer: a paid call against source that exists nowhere in history
        # would be the most expensive way to learn nothing (R3-I02).
        pinned = assert_pinned(ev)
        ev.step("the revisions this PAID run is gating, ASSERTED: both "
                "trees at their pinned commit AND every compiled source "
                "file identical to it",
                byom=pinned["byom"], kovee=pinned["kovee"],
                driver_built_against=pinned["driver_built_against"])
        for provider, (secret, source) in available:
            ctx = None
            tag = f"i1r{provider['kind'][:3]}"
            ev.namespace(f"real-{provider['kind']}")
            try:
                ev.step(f"{provider['kind']}: credential resolved from "
                        f"{source} into the DAEMON's environment only — "
                        f"the binding names it as "
                        f"`env:{provider['env']}`; no key value is in any "
                        f"argument, log or evidence file",
                        provider=provider["kind"], source=source,
                        credential_ref=f"env:{provider['env']}")
                ctx = scripted_flow(
                    ev, tag, provider, {provider["env"]: secret},
                    # Minimal spend: a handful of tokens, once.
                    "Reply with the single word OK.")
                usage = ctx["usage"]
                need(usage["input_tokens"] > 0 and usage["output_tokens"] > 0,
                     f"the provider reported real token counts: {usage}")
                need(ctx["effect"]["transport_profile"] == "https-tls13",
                     f"the effect records the REAL TLS wire: "
                     f"{ctx['effect']}")
                need(ctx["effect"]["provider_ref"],
                     f"the provider's own response id is recorded: "
                     f"{ctx['effect']}")
                # The provider's OWN counts, at every hop.
                effect = ctx["driver"].ok(
                    "effect-show", {"execution_key": ctx["execution_key"]})
                attempt = effect["attempts"][0]
                kovee_report = effect["usage_reports"][0]
                byom_report = ctx["byom"].rows(
                    "SELECT quantities FROM usage_reports")[0]
                byom_settlement = ctx["byom"].rows(
                    "SELECT measured_quantities, charged_quantities"
                    " FROM usage_settlements")[0]
                reported = {q["dimension"]: q["amount"] for q in
                            json.loads(byom_report["quantities"])}
                measured = {q["dimension"]: q["amount"] for q in
                            json.loads(
                                byom_settlement["measured_quantities"])}
                need(attempt["input_tokens"] == usage["input_tokens"]
                     and attempt["output_tokens"] == usage["output_tokens"],
                     f"kovee's attempt row holds the provider's counts: "
                     f"{attempt}")
                need(kovee_report["input_tokens"] == usage["input_tokens"]
                     and kovee_report["output_tokens"]
                     == usage["output_tokens"],
                     f"kovee's usage record holds them: {kovee_report}")
                need(reported == {"input_tokens": usage["input_tokens"],
                                  "output_tokens": usage["output_tokens"]},
                     f"byom's usage_report holds them: {reported}")
                need(measured == reported,
                     f"byom's settlement measured them: {measured}")
                need(json.loads(byom_settlement["charged_quantities"])
                     == [{"dimension": "unit", "unit": "unit",
                          "amount": usage["input_tokens"]
                          + usage["output_tokens"]}],
                     f"byom charged their total: {byom_settlement}")
                # The key is in NO record, on either side.
                leaks = scan_for_secret(
                    [ctx["byom"].data_dir, ctx["kovee"].data_dir,
                     ctx["byom"].channels_dir(), ev.dir], secret)
                need(not leaks,
                     f"the provider credential must appear in no record "
                     f"on either side; found it in {leaks}")
                credential_ref = ctx["kovee"].query(
                    "SELECT credential_secret_ref FROM"
                    " model_provider_bindings WHERE"
                    " model_provider_binding_id = ?",
                    (provider["provider_binding_ref"],))
                need(credential_ref[0]["credential_secret_ref"]
                     == f"env:{provider['env']}",
                     f"kovee records the REFERENCE, never the secret: "
                     f"{credential_ref}")
                per_source_trails(ctx, ev, provider["kind"])
                ev.step(f"{provider['kind']}: a REAL provider call through "
                        "the disclosed metered broker — the provider's OWN "
                        "token counts are identical at every hop: kovee's "
                        "attempt row, kovee's usage record, byom's "
                        "usage_report and byom's measured settlement; the "
                        "credential appears in NO byom record, NO kovee "
                        "record and NO evidence file, and kovee stores "
                        "only the `env:` REFERENCE",
                        provider=provider["kind"],
                        model=ctx["effect"]["model"],
                        provider_ref=ctx["effect"]["provider_ref"],
                        input_tokens=usage["input_tokens"],
                        output_tokens=usage["output_tokens"],
                        charged=ctx["charged"],
                        transport_profile="https-tls13",
                        credential_leaks=[],
                        text=ctx["effect"]["text"])
                honesty_labels(ev, provider,
                               f"a REAL {provider['kind']} call was made "
                               "with synthetic, non-sensitive input; the "
                               "gate claims only that THIS call went "
                               "through the disclosed broker")
            finally:
                cleanup_live()
        print(f"i1-real-model: PASS ({len(available)} provider(s), {ev.n} "
              f"steps; evidence {ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()


# ------------------------------------------------------- harness modes ----

# The harness modes run the SAME loop as --scripted with the AGENT's own
# steps performed by a real Claude Code / Codex session over the real
# byom-mcp and kovee-mcp servers. A governed flow interleaves by
# construction — the agent cannot proceed past an admission or a seat
# position it does not author — so the harness runs as successive
# invocations with the driver's steps in between, exactly as I0 does.
#
# Every argument is fixed by this driver (the exact values --scripted
# sends) and every RESULT is recovered from byomd's OWN records: nothing
# the harness says is evidence.


def harness_prompt(tool: str, args: dict) -> str:
    """The one instruction a session gets. It names where the tool IS, not
    only what to call: codex (0.145, gpt-5.6-sol) does not enumerate MCP
    tools in the model's visible surface — they are reachable only through
    its tool search / dynamic-tool object — and a session that took its
    visible surface for the whole surface answered "that tool is not
    available" and did nothing, which is exactly how the real-codex path
    failed. Claude Code inlines MCP tools, so the sentence is a no-op
    there."""
    return "\n".join([
        "You are the agent participant of a byom Society. The MCP tools "
        "you have been given are your only surface onto it.",
        "",
        f"Call `{tool}` with exactly these arguments. Copy every value "
        "VERBATIM — do not reformat, rename, shorten, invent or omit any "
        "field. Make no other tool call, and do not ask for confirmation.",
        "",
        json.dumps(args, indent=2),
        "",
        f"`{tool}` is served by the MCP server `byom`, which is attached "
        "to this session. If it is not listed in your visible tool "
        "surface, that surface is not the whole surface: your harness may "
        "defer MCP tools behind a tool search or expose them on a dynamic "
        "tools object. Find it there and call it. Never report a byom tool "
        "as unavailable without having searched for it, and never claim a "
        "result you did not receive from the tool itself.",
        "",
        "When it returns, reply with the word DONE followed by the "
        "identifiers it returned.",
    ])


def harness_wire_spec(spec: dict, logs: dict) -> dict:
    """The SAME servers, with `--_mcp-wire` relaying each one's stdio.

    The relay execs exactly the command below with exactly its arguments
    and environment, and passes every byte through untouched — the server
    the harness speaks to is the real binary, and the log is the proof of
    what it was asked and what it answered. Without it a failed call is
    `mcp: byom/<tool> (failed)` and nothing more."""
    return {name: {**server, "command": sys.executable,
                   "args": [str(HERE / "run.py"), "--_mcp-wire",
                            str(logs[name]), server["command"],
                            *server.get("args", [])]}
            for name, server in spec.items()}


def harness_server_spec(byom: ByomDaemon, society: str,
                        salt: str) -> dict:
    """The MCP server configuration BOTH harnesses are given: the real
    byom-mcp binary in its PARTICIPANT profile, byomd's runtime directory,
    the participant channel credential byomd minted, the Society, and the
    session salt.

    The salt is what makes a session's calls NAMEABLE afterwards (R3-I04):
    byom-mcp derives its logical call key from it, byomd records that key
    as the `correlation_ref` of every event the call commits, and the step
    can then ask for the event THIS call produced instead of any event of
    the same kind. It is per session, because a shared salt would make two
    identical calls share an idempotency key."""
    return {"byom": {
        "command": byom_mcp_bin(),
        "args": ["--profile", "participant"],
        "env": {"BYOM_RUNTIME_DIR": str(byom.run_dir),
                "BYOM_PARTICIPANT_TOKEN_FILE":
                    str(byom.token_file(f"participant-{AGENT}.token")),
                "BYOM_SOCIETY": society,
                "BYOM_MCP_SESSION": salt}}}


def harness_launch(which: str, cli: str, prompt: str, server: dict,
                   allowed: str, config_path: Path) -> list:
    """The exact argv the attached harness is launched with. One function,
    used both by the real session and by the deterministic stand-in, so the
    stand-in cannot drift from what `--harness` actually runs."""
    if which == "claude":
        config_path.write_text(json.dumps({"mcpServers": server}, indent=1),
                               encoding="utf-8")
        return [cli, "-p", prompt, "--mcp-config", str(config_path),
                "--strict-mcp-config", "--allowedTools", allowed]
    overrides = []
    for name, spec in server.items():
        key = name.replace("-", "_")
        overrides += ["-c", f"mcp_servers.{key}.command="
                            f"{json.dumps(spec['command'])}"]
        if spec.get("args"):
            overrides += ["-c", f"mcp_servers.{key}.args="
                                f"{json.dumps(spec['args'])}"]
        for ek, evv in spec.get("env", {}).items():
            overrides += ["-c", f"mcp_servers.{key}.env.{ek}="
                                f"{json.dumps(evv)}"]
    # Codex has no per-tool allowlist, so the grant is bounded
    # structurally: with --ignore-user-config the session's ONLY MCP
    # servers are the ones configured here. Both settings are required —
    # an MCP tool call crosses a process boundary that the read-only and
    # workspace-write sandboxes deny.
    return [cli, "exec", "--skip-git-repo-check", "--ignore-user-config",
            "-s", "danger-full-access", "-c", 'approval_policy="never"',
            *overrides, prompt]


# The event byomd commits for each tool the harness drives. It is what
# makes a session's claim CHECKABLE: the step passed only if this event is
# in byomd's ledger AFTER the session and was not there before.
HARNESS_EFFECT = {
    "byom_mandate_prepare": "mandate.prepared",
    "byom_activity_open": "activity.opened",
    "byom_wake_intent_submit": "wake-intent.submitted",
    "byom_pledge_propose": "pledge.proposed",
    "byom_pledge_position": "pledge.position_recorded",
    "byom_pledge_finalize": "pledge.committed",
    "byom_act_intent_prepare": "act-intent.prepared",
    "byom_delivery_submit": "delivery.submitted",
}


def process_group(pgid: int) -> list:
    """Every live process in one process group, named — read from /proc.

    A harness session runs in its own session/group, so whatever is still
    there when the CLI exits is what the CLI leaked; and since byomd binds
    a participant channel to ONE LIVE process, a leaked MCP server is the
    next session's refusal."""
    members = []
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / "stat").read_text(
                encoding="utf-8").rsplit(")", 1)[-1].split()
            if int(fields[2]) != pgid:
                continue
            members.append({
                "pid": int(entry.name), "state": fields[0],
                "cmdline": (entry / "cmdline").read_bytes()
                .decode("utf-8", "replace").replace("\0", " ").strip()})
        except (OSError, IndexError, ValueError):
            continue
    return members


class HarnessAgent:
    """The agent seat, driven by a REAL harness session per step.

    `one(tool, args)` runs one session and then RECOVERS the reply from
    byomd's own store and event ledger — the same members the scripted MCP
    reply would have carried, read from the daemon that committed them.

    A session's own words are never the pass signal. Two things must be
    true of every step: the session's MCP wire shows the tool INVOKED, and
    byomd's ledger holds the effect it commits, minted after the session
    started. A model that narrates `DONE <identifier>` having made no call
    at all — which is exactly how the codex path failed — fails its own
    step, at that step, with the wire to show for it."""

    # A session that made no call — or sent arguments it rewrote — proved
    # nothing about byom: it did not ask this step's question. That is worth
    # asking again (bounded, and every attempt is its own recorded session).
    # A refusal of the EXACT call is never retried: byom answered the
    # governed question, and its answer is the finding.
    ATTEMPTS = 3

    def __init__(self, which: str, cli_path: str, byom: ByomDaemon,
                 society: str, ev: Evidence, workdir: Path, genesis: str):
        self.genesis = genesis
        self.which = which
        self.cli = cli_path
        self.byom = byom
        self.society = society
        self.ev = ev
        self.workdir = workdir
        self.sessions = 0

    def salt(self, n: int) -> str:
        return f"i1-{self.which}-{n:03d}"

    def server(self, n: int) -> dict:
        return harness_server_spec(self.byom, self.society, self.salt(n))

    def session(self, tool: str, args: dict) -> dict:
        """One real CLI session, and everything it can be held to: the
        server's own transcript, the tool surface it was served, and what
        the CLI leaked behind it."""
        self.sessions += 1
        n = self.sessions
        stem = f"session-{n:02d}-{tool}"
        prompt = harness_prompt(tool, args)
        allowed = f"mcp__byom__{tool}"
        spec = self.server(n)
        logs = {name: self.ev.reserve(f"{stem}.{name}-wire.jsonl")
                for name in spec}
        argv = harness_launch(
            self.which, self.cli, prompt, harness_wire_spec(spec, logs),
            allowed, self.ev.path(f"{stem}.mcp.json"))
        started = time.time()
        # Its own session, so the group is exactly this CLI and its
        # descendants: the harness can then END what the CLI left running
        # instead of hoping it exited — byom's channel has ONE live holder.
        proc = subprocess.Popen(argv, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, text=True,
                                cwd=str(self.workdir), start_new_session=True)
        timed_out = False
        try:
            stdout, stderr = proc.communicate(timeout=900)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(proc.pid, signal.SIGKILL)
            stdout, stderr = proc.communicate()
        leaked = self.release_group(proc.pid)
        wire = wire_report(logs["byom"], tool)
        wire["stdout"] = stdout
        self.ev.blob(f"{stem}.txt", "\n".join([
            f"$ {' '.join(argv[:2])} ...",
            f"--- exit {proc.returncode} after {time.time() - started:.1f}s",
            "--- allowed tools", allowed,
            "--- mcp server (relayed verbatim by --_mcp-wire; wire log "
            f"{stem}.byom-wire.jsonl)",
            json.dumps(spec, indent=1),
            "--- byom mcp wire",
            json.dumps({"frames": wire["frames"],
                        "tools_served": len(wire["served"]),
                        f"{tool}_served": tool in wire["served"],
                        "invocations": wire["invocations"],
                        "other_calls": wire["other_calls"],
                        "server_stderr": wire["stderr"]}, indent=1),
            "--- processes the CLI left running (ended by the harness)",
            json.dumps(leaked, indent=1),
            f"--- stdout\n{stdout}",
            f"--- stderr\n{stderr}"]))
        need(not timed_out,
             f"{self.which} session {n:02d} ({tool}) never returned")
        need(proc.returncode == 0,
             f"{self.which} session {n:02d} ({tool}) failed "
             f"({proc.returncode}): {stderr[-600:]}")
        # The surface question, answered from the SERVER's own reply and
        # not from the model's account of it.
        need(wire["served"],
             f"{self.which} session {n:02d}: the byom MCP server was never "
             f"asked for its tools (wire {stem}.byom-wire.jsonl): "
             f"{wire['stderr'][-3:]}")
        need(tool in wire["served"],
             f"{self.which} session {n:02d}: byom-mcp did not advertise "
             f"{tool}, which the harness allowed — served "
             f"{len(wire['served'])} tools: {sorted(wire['served'])}")
        need(not wire["other_calls"],
             f"{self.which} session {n:02d} called tools the prompt did "
             f"not name: {[c['name'] for c in wire['other_calls']]}")
        wire["salt"] = self.salt(n)
        return wire

    def release_group(self, pgid: int) -> list:
        """End whatever the CLI session left behind, and say what it was.

        The CLI exiting is not the same as its MCP servers exiting, and a
        surviving byom-mcp still HOLDS the agent's participant channel
        (byomd refuses a second live claimant, `channel.rs`), so the next
        session would be refused for a reason no evidence would name."""
        leaked = [p for p in process_group(pgid) if p["pid"] != pgid]
        if not leaked:
            return []
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try:
                os.killpg(pgid, sig)
            except (ProcessLookupError, PermissionError):
                break
            deadline = time.time() + 5
            while time.time() < deadline and process_group(pgid):
                time.sleep(0.02)
            if not process_group(pgid):
                break
        return leaked

    # The ONE agent call the harness cannot supply the result of.
    # `episode_request` publishes byom's stage-3 allocation pin only in its
    # REPLY (byom seam finding S-1: the store's keyed record commitment is
    # byom's own and is never asked for), and nothing a harness says is
    # evidence — so this call rides the scenario's own short-lived
    # agent-channel client instead. It is the same participant channel, the
    # same operation and the same arguments; only the caller differs, and
    # the evidence says so.
    SCENARIO_DRIVEN = {"byom_episode_request"}

    def one(self, tool: str, args: dict, frames: str | None = None) -> dict:
        if tool in self.SCENARIO_DRIVEN:
            reply = agent_socket_call(self.byom, {
                "version": "0.2", "op": tool[len("byom_"):],
                "meta": meta(self.byom.incarnation(),
                             f"harness-{tool}-{self.sessions}"),
                **args})
            need(reply.get("outcome") == "ok", f"{tool}: {reply}")
            self.ev.blob(f"scenario-driven-{tool}.json",
                         json.dumps({"why": "byom publishes the stage-3 "
                                            "allocation pin only in the "
                                            "reply (S-1), and a harness "
                                            "reply is never evidence",
                                     "reply": reply}, indent=1))
            return reply
        need(tool in HARNESS_EFFECT,
             f"no byom event is pinned for {tool}, so a session driving it "
             f"could not be held to anything")
        kind = HARNESS_EFFECT[tool]
        want = canonical(args)
        idle: list = []
        for _ in range(self.ATTEMPTS):
            since = len(timeline(self.byom, self.genesis))
            wire = self.session(tool, args)
            stem = f"session-{self.sessions:02d}-{tool}"
            # "Every argument is fixed by this driver" is a CLAIM until the
            # RAW BYTES the server was sent are compared with what the
            # driver fixed. R3-I04(a): this used to compare parsed dicts, so
            # `1.0` passed for `1` and a duplicate key lost a value
            # silently. The comparison is now byte equality of a canonical
            # form that keeps number spelling and refuses duplicate keys.
            exact = [c for c in wire["invocations"]
                     if c["canonical_arguments"] == want]
            altered = [c for c in wire["invocations"]
                       if c["canonical_arguments"] != want]
            # R3-I04(b): correlate by REQUEST IDENTITY, not by event kind.
            # byom-mcp derives its request_id from this session's salt, the
            # tool and the JCS of the arguments byomd was actually sent, and
            # byomd stores it as the event's `correlation_ref` — so the only
            # event that can pass this step is the one THIS call committed.
            # Kind-correlation let any same-kind event landing after the
            # mark satisfy a step whose exact invocation byom had REFUSED.
            wanted_ref = correlation_of(wire["salt"], tool, args)
            mine = [e for e in self.since(kind, since)
                    if e.get("correlation_ref") == wanted_ref]
            others = [e for e in self.since(kind, since)
                      if e.get("correlation_ref") != wanted_ref]
            # A refusal of the EXACT call is the finding, whatever else
            # landed: byom answered the governed question. It can no longer
            # be masked by an unrelated event of the same kind.
            refused = [c for c in exact if c["failed"]]
            if refused:
                raise Fail(
                    f"{self.which} session {self.sessions:02d} called {tool} "
                    f"with the exact arguments this driver fixed and byom "
                    f"REFUSED it — so byomd committed no {kind} FOR THIS "
                    f"CALL ({wanted_ref}), whatever else is on the ledger "
                    f"({len(others)} other {kind} event(s) after the mark). "
                    f"The server's answer, verbatim: "
                    f"{refused[-1]['answer'][:900]} (full wire: "
                    f"{stem}.byom-wire.jsonl)")
            if mine:
                need(exact,
                     f"{self.which} session {self.sessions:02d}: byomd holds "
                     f"a new {kind} event correlated to this call, but no "
                     f"invocation on this session's wire carried the exact "
                     f"bytes this driver fixed — the effect is not this "
                     f"step's. Sent: "
                     f"{json.dumps([c['raw_arguments'] for c in altered])[:600]} "
                     f"({stem}.byom-wire.jsonl)")
                need(not altered,
                     f"{self.which} session {self.sessions:02d} also called "
                     f"{tool} with arguments it changed: "
                     f"{json.dumps([c['raw_arguments'] for c in altered])[:600]}")
                self.mark = since
                self.correlation = wanted_ref
                return self.recover(tool, args)
            # Either no call at all, a call the model rewrote, or a
            # same-kind event that some OTHER call committed: in every case
            # this session did not perform the step, and its words are not
            # evidence of anything however confident they sound. Worth one
            # more session — bounded, and every attempt is recorded.
            idle.append({
                "session": self.sessions,
                "salt": wire["salt"],
                "invocations": len(wire["invocations"]),
                "wanted_correlation_ref": wanted_ref,
                "uncorrelated_same_kind_events": len(others),
                "altered_raw_arguments": [c["raw_arguments"]
                                          for c in altered],
                "canonical_errors": [c.get("canonical_error")
                                     for c in wire["invocations"]
                                     if c.get("canonical_error")],
                "server_answer": [c["answer"][:300] for c in altered],
                "said": (wire["stdout"] or "").strip()[:300]})
        raise Fail(
            f"{self.ATTEMPTS} {self.which} sessions failed to drive {tool} "
            f"and byomd committed no {kind} correlated to any of them: "
            f"byom-mcp advertised the tool in every one of them and the "
            f"driver's arguments were in every prompt, so the sessions "
            f"either made no call or rewrote it before sending. Expected "
            f"bytes: {want[:400]}. Per session: {json.dumps(idle)[:1500]}")

    def open(self) -> "HarnessAgent":
        return self

    def call_ok(self, tool: str, args: dict) -> dict:
        return self.one(tool, args)

    def close(self, frames: str | None = None):
        return None

    # -- recovery: byomd's own records, never the session's words --------

    # Where the current step's session began in byomd's ledger, and the
    # `correlation_ref` of the call that passed it. Recovery reads only
    # what was minted after the mark AND correlated to this exact call, so
    # a step can be passed neither by an event an earlier session (or the
    # scenario) committed, nor by an unrelated same-kind event of this one.
    mark = 0
    correlation: str | None = None

    def since(self, kind: str, mark: int) -> list:
        return [e for i, e in enumerate(timeline(self.byom, self.genesis))
                if e["kind"] == kind and i >= mark]

    def last(self, kind: str) -> dict:
        rows = [e for e in self.since(kind, self.mark)
                if self.correlation is None
                or e.get("correlation_ref") == self.correlation]
        need(rows, f"byomd's ledger holds no {kind} event correlated to "
                   f"this step's own call ({self.correlation})")
        return rows[-1]

    def recover(self, tool: str, args: dict) -> dict:
        byom = self.byom
        if tool == "byom_mandate_prepare":
            mandate = self.last("mandate.prepared")["object_ref"]
            row = byom.rows("SELECT subject_digest, required_seat_refs"
                            " FROM mandates WHERE mandate_id = ?",
                            (mandate,))[0]
            return {"result": {
                "mandate_id": mandate,
                "subject_digest": json.loads(row["subject_digest"]),
                "required_seat_refs": [
                    s["seat_ref"] for s in
                    json.loads(row["required_seat_refs"])]}}
        if tool == "byom_activity_open":
            stream = self.last("activity.opened")["object_ref"]
            return {"result": {
                "activity_stream_id": stream,
                "state": byom.row("SELECT state FROM activity_streams"
                                  " WHERE activity_stream_id = ?", stream)}}
        if tool == "byom_wake_intent_submit":
            wake = self.last("wake-intent.submitted")["object_ref"]
            return {"result": {"wake_intent_id": wake,
                               "state": byom.row(
                                   "SELECT state FROM wake_intents"
                                   " WHERE wake_intent_id = ?", wake)}}
        if tool == "byom_pledge_propose":
            proposal = self.last("pledge.proposed")["object_ref"]
            row = byom.rows("SELECT terms_digest, required_slots FROM"
                            " pledge_proposals WHERE proposal_id = ?",
                            (proposal,))[0]
            slots = json.loads(row["required_slots"])["seats"]
            return {"result": {
                "proposal_id": proposal,
                "terms_digest": json.loads(row["terms_digest"]),
                "required_slots": [{"kind": s["kind"],
                                    "seat_refs": [s["seat_ref"]]}
                                   for s in slots]}}
        if tool == "byom_pledge_position":
            # This step used to return a CONSTANT: `{"state": "recorded"}`,
            # asserted against nothing. A codex session that made no call at
            # all therefore passed it, and the flow only broke three steps
            # later, at a finalize byom was right to refuse (the pledgor seat
            # held no assent). The Position byomd actually recorded is the
            # only thing that can pass this step.
            recorded = self.last("pledge.position_recorded")
            need(recorded["object_ref"] == args["proposal_ref"],
                 f"the new pledge position is on {recorded['object_ref']}, "
                 f"not the proposal this step positions "
                 f"({args['proposal_ref']})")
            # The seat HEAD is what byom itself reads to decide whether the
            # required seat set assented (`all_seats_assent`), so it is what
            # this step must be held to — and the head's own revision row
            # says who authored it.
            heads = byom.rows(
                "SELECT position_ref, value, status FROM position_seat_heads"
                " WHERE proposal_kind = 'pledge' AND proposal_ref = ?"
                " AND seat_ref = ?",
                (args["proposal_ref"], args["seat_ref"]))
            need(heads,
                 f"byomd holds no seat head for {args['seat_ref']} on "
                 f"{args['proposal_ref']}: the seat is unfilled")
            head = heads[0]
            row = byom.rows("SELECT participant_ref, proposal_revision,"
                            " assent_mode FROM position_revisions"
                            " WHERE position_id = ?",
                            (head["position_ref"],))[0]
            need(head["status"] == "active"
                 and head["value"] == args["value"]
                 and row["participant_ref"] == AGENT,
                 f"byomd recorded no active {args['value']} by {AGENT} for "
                 f"seat {args['seat_ref']}: {head} {row}")
            return {"result": {"position_id": head["position_ref"],
                               "state": "recorded",
                               "assent_mode": row["assent_mode"],
                               "revision": row["proposal_revision"]}}
        if tool == "byom_pledge_finalize":
            return {"result": {
                "pledge_id": self.last("pledge.committed")["object_ref"]}}
        if tool == "byom_act_intent_prepare":
            intent = self.last("act-intent.prepared")["object_ref"]
            row = byom.rows(
                "SELECT subject_digest, required_seat_refs,"
                " stable_execution_key, budget_reservation_set_ref,"
                " act_class_subject, act_class FROM act_intents"
                " WHERE intent_id = ?", (intent,))[0]
            return {"result": {
                "intent_id": intent,
                "act_class": row["act_class"],
                "subject_digest": json.loads(row["subject_digest"]),
                "required_seat_refs": [
                    s["seat_ref"] for s in
                    json.loads(row["required_seat_refs"])],
                "stable_execution_key": row["stable_execution_key"],
                "budget_reservation_set_ref":
                    row["budget_reservation_set_ref"],
                "act_class_subject": json.loads(row["act_class_subject"])}}
        if tool == "byom_delivery_submit":
            delivery = self.last("delivery.submitted")["object_ref"]
            return {"result": {"delivery_id": delivery}}
        raise Fail(f"the harness recovery map has no entry for {tool}")


# The participant tool surface both harnesses see, captured once per
# attached-path cell and compared BETWEEN them: "same tool schemas, zero
# server-side changes" (plan §8) is then a comparison, not a claim.
_TOOL_SURFACE: dict[str, str] = {}


class AttachedStandIn(AgentChannel):
    """The attached harness's execution path, driven deterministically.

    Every agent step goes through the REAL byom-mcp binary over real MCP
    stdio frames — the surface a Claude Code or Codex session speaks — and
    for each step this also:

      * asserts the tool the harness would be allowed EXISTS in the
        server's own `tools/list`, with the input schema it would send
        arguments against (so a real session cannot fail on a name or a
        shape this gate never checked);
      * constructs the exact launch argv `--harness <which>` would use, by
        the same function, and records it as evidence.

    What it does NOT do is run the model: no CLI is invoked, so this cell
    is deterministic and gates CI, while `--harness <which>` runs the real
    session under I1_REAL_HARNESS=1. The evidence says which one ran."""

    def __init__(self, which: str, byom: ByomDaemon, society: str,
                 ev: Evidence, genesis: str | None = None):
        super().__init__(byom, society, ev)
        self.which = which
        self.tag = which
        self.genesis = genesis
        self.cli = shutil.which(which) or f"<{which}: not on PATH>"
        self.steps: list = []
        self.tools: dict = {}
        self.correlations: list = []

    def one(self, tool: str, arguments: dict, frames: str | None = None):
        mcp = self.open()
        salt = self.salt()
        try:
            if not self.tools:
                listed = mcp.tools()
                self.tools = {t["name"]: t for t in listed}
                surface = hashlib.sha256(jcs(sorted(
                    (t["name"], t.get("inputSchema") or t.get("input_schema"))
                    for t in listed))).hexdigest()
                previous = _TOOL_SURFACE.setdefault(self.which, surface)
                need(previous == surface,
                     "the participant tool surface changed within a run")
                other = [v for k, v in _TOOL_SURFACE.items()
                         if k != self.which]
                need(all(v == surface for v in other),
                     "the two attached harnesses must see the IDENTICAL "
                     "tool surface: zero server-side changes (plan §8)")
                self.ev.blob("participant-tools.json", json.dumps(
                    {"count": len(listed), "surface_sha256": surface,
                     "names": sorted(self.tools)}, indent=1))
            need(tool in self.tools,
                 f"{self.which} would be allowed {tool}, which byom-mcp "
                 f"does not serve: {sorted(self.tools)}")
            schema = (self.tools[tool].get("inputSchema")
                      or self.tools[tool].get("input_schema") or {})
            need(schema.get("type") == "object" and "properties" in schema,
                 f"{tool} has no object input schema: {schema}")
            unknown = [k for k in arguments
                       if k not in (schema.get("properties") or {})]
            need(not unknown,
                 f"{tool}: the arguments this gate sends are not in the "
                 f"schema the harness would see: {unknown}")
            argv = harness_launch(
                self.which, self.cli, harness_prompt(tool, arguments),
                harness_server_spec(self.byom, self.society, salt),
                f"mcp__byom__{tool}",
                self.ev.path(f"launch-{len(self.steps) + 1:02d}-{tool}"
                             ".mcp.json"))
            self.steps.append({"tool": tool, "allowed": f"mcp__byom__{tool}",
                               "argv_prefix": argv[:2],
                               "argv_length": len(argv)})
            reply = mcp.call_ok(tool, arguments)
            self.check_correlation(tool, arguments, salt)
            return reply
        finally:
            mcp.close(frames)

    def check_correlation(self, tool: str, arguments: dict, salt: str):
        """The real-harness oracle's identity correlation, DETERMINISTICALLY
        exercised here (R3-I04).

        `HarnessAgent` decides whether a session drove its step by asking
        byomd for the event whose `correlation_ref` is the byom-mcp logical
        call key of THIS call — the only thing that distinguishes it from
        another call of the same kind. That derivation lives in this file
        and byom-mcp's `bridge.rs::meta` computes the real one, so it is a
        cross-repo agreement, and nothing deterministic used to check it:
        the whole oracle first ran in a 40-minute real-CLI mode.

        Here it is checked on every attached step: byomd's newest event of
        the tool's pinned kind must carry exactly the ref this file
        derived."""
        if self.genesis is None or tool not in HARNESS_EFFECT:
            return
        kind = HARNESS_EFFECT[tool]
        expected = correlation_of(salt, tool, arguments)
        events = [e for e in timeline(self.byom, self.genesis)
                  if e["kind"] == kind]
        need(events, f"byomd committed no {kind} for {tool}")
        need(events[-1].get("correlation_ref") == expected,
             f"the byom-mcp logical call key this file derives is not the "
             f"one byomd recorded for {tool}: derived {expected}, byomd "
             f"holds {events[-1].get('correlation_ref')}. The real-harness "
             f"oracle correlates a session's call with byomd's ledger by "
             f"exactly this value, so a drift here would silently weaken it")
        self.correlations.append({"tool": tool, "salt": salt,
                                  "correlation_ref": expected,
                                  "event_id": events[-1]["event_id"]})

# ------------------------------------------- the oracle, tested itself ----
#
# R3-I04: nothing deterministic exercised the real-harness oracle at all —
# not the relay's recording, not the mark, not the retry bound, not the
# gate in front of recovery. `AttachedStandIn` is a different implementation
# (direct MCP), so the first time `HarnessAgent`'s logic ran was inside a
# forty-minute real-CLI mode, and its two false-positive paths were found by
# an external probe rather than by this gate.
#
# What follows is that probe, shipped: the REAL relay recording a REAL stdio
# round trip, and the REAL `wire_report`/`HarnessAgent.one` driven over
# wires this cell composes. Only the CLI session and byomd are stood in for.

ORACLE_ECHO_SERVER = '''\
import json, sys
for line in sys.stdin:
    if not line.strip():
        continue
    frame = json.loads(line)
    if frame.get("method") == "tools/list":
        body = {"tools": [{"name": "byom_activity_open",
                           "inputSchema": {"type": "object",
                                           "properties": {}}}]}
    elif frame.get("method") == "tools/call":
        # The arguments are echoed back VERBATIM, so a reader can see what
        # this server was actually handed.
        body = {"content": [{"type": "text",
                             "text": json.dumps(frame["params"]["arguments"])}]}
    elif frame.get("method") == "initialize":
        body = {"protocolVersion": "2025-06-18"}
    else:
        continue
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": frame.get("id"),
                                 "result": body}) + "\\n")
    sys.stdout.flush()
'''


def oracle_relay_roundtrip(ev: Evidence) -> dict:
    """`--_mcp-wire` relaying a real stdio server, so the RECORDING half of
    the oracle is exercised rather than assumed.

    The frame sent here is spelled the way a model might spell it — members
    reordered, whitespace, `1.0` where the driver fixed `1`. The relay must
    hand the server those exact bytes and keep them, and `wire_report` must
    read them back byte for byte."""
    log = ev.reserve("relay-roundtrip.jsonl")
    ev.blob("echo-server.py", ORACLE_ECHO_SERVER)
    server = ev.path("echo-server.py")
    sent = ('{"jsonrpc":"2.0","id":2,"method":"tools/call","params":'
            '{"name":"byom_activity_open","arguments":'
            '{ "generation" : 1.0 , "kind" : "exploration" }}}')
    relay = subprocess.run(
        [sys.executable, str(HERE / "run.py"), "--_mcp-wire", str(log),
         sys.executable, str(server)],
        input=('{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}\n'
               + sent + "\n"),
        capture_output=True, text=True, timeout=60)
    need(relay.returncode == 0,
         f"the relay child failed ({relay.returncode}): {relay.stderr[-400:]}")
    report = wire_report(log, "byom_activity_open")
    need(len(report["invocations"]) == 1,
         f"the relay recorded the call: {report}")
    call = report["invocations"][0]
    need(call["raw_arguments"]
         == '{ "generation" : 1.0 , "kind" : "exploration" }',
         f"the RAW argument bytes the server was sent are what the relay "
         f"kept, whitespace and all: {call['raw_arguments']!r}")
    need(call["canonical_arguments"]
         == '{"generation":1.0,"kind":"exploration"}',
         f"and their canonical form keeps the number as it was spelled: "
         f"{call['canonical_arguments']!r}")
    need(call["canonical_arguments"] != canonical({"generation": 1,
                                                   "kind": "exploration"}),
         "which is exactly the distinction the parsed-dict comparison lost")
    need(json.loads(call["answer"]) == {"generation": 1.0,
                                        "kind": "exploration"},
         f"the server answered over the same relay: {call['answer']!r}")
    return {"relay_log": str(log.relative_to(ev.dir)),
            "raw_arguments": call["raw_arguments"],
            "canonical_arguments": call["canonical_arguments"],
            "driver_would_have_sent": canonical({"generation": 1,
                                                 "kind": "exploration"}),
            "frames": report["frames"]}


class OracleByom:
    """A byomd stand-in with a BEFORE and an AFTER ledger, so the real
    `timeline()` and `HarnessAgent.since()` run unmodified and the mark
    means what it means in a live run."""

    def __init__(self, before: list, after: list):
        self.events = list(before)
        self.after = list(after)

    def expect_ok(self, surface: str, request: dict) -> dict:
        return {"result": {"events": self.events}}


class OracleProbe(HarnessAgent):
    """`HarnessAgent`'s own logic, over wires this cell composes.

    Everything under test is the SHIPPED code: `one()`, the canonical byte
    comparison, the correlation by request identity, the mark, the retry
    bound and the gate in front of `recover()`."""

    def __init__(self, ev: Evidence, label: str, wires: list,
                 before: list, after: list):
        self.which = "probe"
        self.ev = ev
        self.label = label
        self.sessions = 0
        self.wires = list(wires)
        self.byom = OracleByom(before, after)
        self.genesis = "cursor-probe"
        self.recovered = None

    def salt(self, n: int) -> str:
        return f"probe-{self.label}-{n:03d}"

    def session(self, tool: str, args: dict) -> dict:
        self.sessions += 1
        n = self.sessions
        calls = self.wires[n - 1] if n <= len(self.wires) else []
        log = self.ev.reserve(f"probe-{self.label}-{n:02d}.jsonl")
        listed = {"jsonrpc": "2.0", "id": 1,
                  "result": {"tools": [{"name": tool}]}}
        lines = [json.dumps({"at": 0, "dir": "relay", "argv": ["probe"]}),
                 json.dumps({"at": 0, "dir": "server->harness",
                             "frame": listed, "raw": json.dumps(listed)})]
        for i, (raw_arguments, failed) in enumerate(calls, start=2):
            request = ('{"jsonrpc":"2.0","id":%d,"method":"tools/call",'
                       '"params":{"name":%s,"arguments":%s}}'
                       % (i, json.dumps(tool), raw_arguments))
            answer = {"jsonrpc": "2.0", "id": i,
                      "result": {"content": [{"type": "text", "text":
                                              "byom refused: the mandate "
                                              "does not permit this"
                                              if failed else "{}"}],
                                 "isError": failed}}
            lines.append(json.dumps({"at": 0, "dir": "harness->server",
                                     "frame": json.loads(request),
                                     "raw": request}))
            lines.append(json.dumps({"at": 0, "dir": "server->harness",
                                     "frame": answer,
                                     "raw": json.dumps(answer)}))
        log.write_text("\n".join(lines) + "\n", encoding="utf-8")
        # Whatever the session did or did not do, byomd's ledger is now
        # whatever this case says it is.
        self.byom.events = list(self.byom.after)
        wire = wire_report(log, tool)
        wire["salt"] = self.salt(n)
        wire["stdout"] = "DONE act-0001 — the session's own words"
        return wire

    def recover(self, tool: str, args: dict) -> dict:
        self.recovered = {"tool": tool, "correlation": self.correlation,
                          "mark": self.mark}
        return {"result": self.recovered}


def oracle_self_test(ev: Evidence) -> None:
    """The real-harness oracle, held to its own failure modes — the two an
    external probe found, and the ones it confirmed were already closed.

    Each case is a mutation of the one case that must pass, so a weakening
    of the oracle turns one of them green."""
    ev.namespace("oracle-self-test")
    tool = "byom_activity_open"
    kind = HARNESS_EFFECT[tool]
    args = {"generation": 1, "kind": "exploration"}
    relay = oracle_relay_roundtrip(ev)
    ev.step("R3-I04(a): the relay keeps the BYTES. `--_mcp-wire` was run "
            "over a real stdio server and handed it a frame spelled the way "
            "a model might — members reordered, whitespace, `1.0` where the "
            "driver fixed `1` — and the recorded log holds those exact "
            "bytes, which `wire_report` reads back and canonicalises "
            "WITHOUT losing the number's spelling. The relay used to "
            "`json.loads` each frame and throw the text away, so the "
            "oracle's `byte-equal` comparison was really Python equality "
            "between two parsed dicts",
            **relay)

    exact = '{"generation":1,"kind":"exploration"}'
    reordered = '{ "kind": "exploration",\n  "generation": 1 }'
    respelled = '{"generation":1.0,"kind":"exploration"}'
    shortened = '{"kind":"exploration"}'

    def mine(label: str, n: int = 1) -> dict:
        """The event THIS call would commit: correlated by the byom-mcp
        logical call key, exactly as byomd records it."""
        return {"kind": kind, "event_id": f"evt-{label}", "object_ref": "act-1",
                "correlation_ref": correlation_of(
                    f"probe-{label}-{n:03d}", tool, args)}

    someone_else = {"kind": kind, "event_id": "evt-another-call",
                    "object_ref": "act-9999",
                    "correlation_ref": "req-an-entirely-different-call"}
    cases = []

    def case(label: str, wires: list, after: list, expect: str,
             because: str, before: list | None = None):
        probe = OracleProbe(ev, label, wires, before or [], after)
        try:
            probe.one(tool, args)
            got, detail = "RECOVERED", json.dumps(probe.recovered)
        except Fail as refusal:
            got, detail = "RED", str(refusal)
        need(got == expect,
             f"oracle self-test {label!r}: expected {expect}, got {got} — "
             f"{because}. Detail: {detail[:700]}")
        cases.append({"case": label, "expected": expect, "got": got,
                      "sessions": probe.sessions, "why": because,
                      "detail": detail[:400]})

    # The one case that must PASS: the exact bytes, and byomd holding the
    # event THIS call committed.
    case("exact-call-recovers", [[(exact, False)]],
         [mine("exact-call-recovers")], "RECOVERED",
         "the session sent the exact bytes and byomd holds the event that "
         "call committed, so the step is done")

    # R3-I04(a). The event is even correlated to this call — the ONLY thing
    # that makes this red is the byte comparison.
    case("respelled-number-is-red",
         [[(respelled, False)]] * HarnessAgent.ATTEMPTS,
         [mine("respelled-number-is-red")], "RED",
         "`1.0` is not `1` on the wire, and the old parsed-dict comparison "
         "accepted it")

    # Member ORDER carries no JSON meaning, and the gate says so rather
    # than claiming a strictness it does not have.
    case("reordered-members-recover", [[(reordered, False)]],
         [mine("reordered-members-recover")], "RECOVERED",
         "object member order is not a JSON distinction: the same members "
         "in another order ARE the driver's arguments")

    # R3-I04(b): an exact REFUSAL is the finding, and an unrelated same-kind
    # event landing after the mark can no longer mask it.
    case("exact-refusal-plus-unrelated-event-is-red", [[(exact, True)]],
         [someone_else], "RED",
         "byom answered the exact governed question with a refusal, and a "
         "same-kind event some other call committed is not this step's")

    case("exact-refusal-alone-is-red", [[(exact, True)]], [], "RED",
         "an exact refusal is never retried and never recovered")

    case("no-call-at-all-is-red", [[], [], []], [], "RED",
         "a session that narrates DONE without calling anything proves "
         "nothing, and is retried a bounded number of times")

    case("shortened-arguments-are-red",
         [[(shortened, False)]] * HarnessAgent.ATTEMPTS, [someone_else],
         "RED",
         "a session that dropped a member did not ask this step's question, "
         "whatever else landed on the ledger")

    # The mark: an event already on the ledger before the session cannot
    # pass it, even though it is correlated to this very call.
    case("pre-mark-event-is-red", [[(exact, False)]],
         [mine("pre-mark-event-is-red")], "RED",
         "the mark is the pre-session timeline length, so an event that was "
         "already there is excluded",
         before=[mine("pre-mark-event-is-red")])

    ev.blob("oracle-self-test.json", json.dumps(cases, indent=1))
    ev.step("R3-I04: the real-harness oracle, driven DETERMINISTICALLY over "
            "its own failure modes — the shipped `HarnessAgent.one()`, the "
            "shipped `wire_report`, the shipped canonical byte comparison "
            "and the shipped correlation by request identity, with only the "
            "CLI session and byomd stood in for. Two cases must pass and "
            "six must go RED, including the two false positives an external "
            "probe found: a respelled number accepted as the driver's "
            "argument, and an exact REFUSED invocation masked by an "
            "unrelated same-kind event after the mark",
            cases=cases,
            attempts_before_giving_up=HarnessAgent.ATTEMPTS,
            recovered=[c["case"] for c in cases if c["got"] == "RECOVERED"],
            red=[c["case"] for c in cases if c["got"] == "RED"])




def mode_attached_path(which: str) -> int:
    """i1-attached-<which>: plan §8's attached execution path, gated.

    R3-I01 (e): `--all-checks` used to exclude the harness paths outright,
    so two of the plan's three execution paths were not gated at all. This
    cell gates the attached path deterministically — the real byom-mcp
    surface, the harness's own allowlist and launch configuration, the
    whole governed loop through it — and `--harness <which>` remains the
    env-gated real session on top."""
    test_id = f"i1-attached-{which}"
    ev = Evidence(test_id)
    print(f"{test_id}: plan §8 execution path — ATTACHED {which}, driven "
          "deterministically over the real byom-mcp stdio surface "
          f"(the real {which} CLI session is `--harness {which}`, gated by "
          "I1_REAL_HARNESS=1)")
    ctx = None
    try:
        pinned = assert_pinned(ev)
        ev.step("the revisions this path is running, ASSERTED (R3-I02)",
                byom=pinned["byom"], kovee=pinned["kovee"],
                driver_built_against=pinned["driver_built_against"])
        ev.namespace(f"attached-{which}")
        ctx = scripted_flow(
            ev, f"i1a{which[:2]}", RECORDING,
            {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY}, "Say OK.",
            agent_factory=lambda byom, society, e, genesis: AttachedStandIn(
                which, byom, society, e, genesis))
        per_source_trails(ctx, ev, which)
        agent = ctx["agent"]
        correlated = [s for s in agent.steps if s["tool"] in HARNESS_EFFECT]
        need(len(agent.correlations) == len(correlated) >= 6,
             f"the real-harness oracle's identity correlation must be "
             f"checked on EVERY attached step whose tool has a pinned byom "
             f"event: {len(agent.correlations)} checked of "
             f"{len(correlated)} such steps")
        ev.blob("oracle-correlations.json",
                json.dumps(agent.correlations, indent=1))
        need(len(agent.steps) >= 8,
             f"the attached path must drive the agent's own steps: "
             f"{agent.steps}")
        need(len(agent.tools) == 34,
             f"the participant profile serves 34 tools: {len(agent.tools)}")
        ev.blob("attached-steps.json", json.dumps(agent.steps, indent=1))
        honesty_labels(ev, RECORDING,
                       f"the agent half went through the real byom-mcp "
                       f"PARTICIPANT surface with the exact tool allowlist "
                       f"and launch configuration `--harness {which}` uses; "
                       f"no {which} CLI was invoked in this cell, so it is "
                       f"deterministic — the real session is env-gated")
        ev.step(f"plan §8 execution path {2 if which == 'claude' else 3}/3 "
                f"— ATTACHED {which}: every agent step of the governed loop "
                "went through the REAL byom-mcp participant surface; each "
                "tool the harness would be allowed exists in the server's "
                "own tools/list with the object input schema the arguments "
                "are sent against; the exact launch argv is built by the "
                "same function `--harness` uses; and the tool surface is "
                "byte-identical to the other harness's — zero server-side "
                "changes",
                harness=which, agent_steps=len(agent.steps),
                tools_served=len(agent.tools),
                tool_surface_sha256=_TOOL_SURFACE[which],
                cli_on_path=shutil.which(which) is not None,
                real_session_mode=f"--harness {which} (I1_REAL_HARNESS=1)",
                # R3-I04: the real-harness oracle decides a step by asking
                # byomd for the event whose `correlation_ref` is the
                # byom-mcp logical call key of THAT call. The derivation
                # lives in this file and byom-mcp computes the real one, so
                # it is a cross-repo agreement — checked here, on every
                # attached step whose tool has a pinned byom event, against
                # byomd's own record.
                oracle_correlations_checked=[c["tool"]
                                             for c in agent.correlations])
        rows = plan_coverage(ev, {
            f"attached-{which}": "the deterministic attached path, over the "
                                 "real byom-mcp participant surface"})
        print_coverage(rows, f"{test_id}: plan §8 I1 coverage (this mode):")
        print(f"{test_id}: PASS ({ev.n} steps; evidence "
              f"{ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()
        cleanup_live()


def harness_instructions(which: str) -> str:
    return f"""\
Spawn the daemons first (isolated dirs), establish the Society and the
offer, then register the MCP servers with the harness:

  byomd:  BYOM_DATA_DIR=<data-dir> BYOM_RUNTIME_DIR=<run-dir> {byomd_bin()}
  koveed: KOVEE_RUNTIME_DIR=<kovee-run> KOVEE_BYOM_RUNTIME_DIR=<run-dir> \\
          KOVEE_BYOM_CHANNELS_DIR=<data-dir>/channels {koveed_bin()} \\
          --data-dir <kovee-data>

The harness then drives the AGENT half of the I1 loop — mandate_prepare,
activity_open, wake_intent_submit, pledge_propose, the pledgor seat's
pledge_position, pledge_finalize, act_intent_prepare, activity_open
(pledge_work) and delivery_submit — while the governance and human-seat
steps (both admissions, the mandate seat, the act GATE seat, call_open,
the beneficiary seat, review_record), kovee's own steps and the placement
and broker steps stay with this scenario. Every result is recovered from
byomd's own event ledger and store; nothing the harness says is evidence.

  {"claude mcp add byom --env BYOM_RUNTIME_DIR=<run-dir> ..."
   if which == "claude" else
   "[mcp_servers.byom] command/args/env in ~/.codex/config.toml ..."}
"""


def mode_harness(which: str) -> int:
    test_id = f"i1-flow-{which}"
    if os.environ.get("I1_REAL_HARNESS") != "1":
        print(f"{test_id}: SKIP (env-gated; set I1_REAL_HARNESS=1 to run a "
              "real harness session). Setup:\n")
        print(harness_instructions(which))
        return 2
    harness_cli = shutil.which(which)
    if harness_cli is None:
        print(f"{test_id}: SKIP — I1_REAL_HARNESS=1 but no `{which}` CLI on "
              "PATH. Setup:\n")
        print(harness_instructions(which))
        return 2
    ev = Evidence(test_id)
    print(f"{test_id}: a real {which} session drives each AGENT step of the "
          "I1 loop over the real byom-mcp; every result is verified from "
          "byomd's own records")
    ev.blob("setup-instructions.txt", harness_instructions(which))
    workdir = Path(tempfile.mkdtemp(prefix=f"i1-harness-{which}-cwd-"))
    ctx = None
    try:
        # R3-I02: the real-harness modes used not to pin anything at all —
        # the one place a 40-minute run against the wrong source would cost
        # the most.
        pinned = assert_pinned(ev)
        ev.step("the revisions this real-harness run is gating, ASSERTED "
                "(R3-I02): both trees at their pinned commit AND every "
                "compiled source file identical to it",
                byom=pinned["byom"], kovee=pinned["kovee"],
                driver_built_against=pinned["driver_built_against"])
        ev.namespace(f"harness-{which}")
        ctx = scripted_flow(
            ev, f"i1h{which[:2]}", RECORDING,
            {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY}, "Say OK.",
            agent_factory=lambda byom, society, e, genesis: HarnessAgent(
                which, harness_cli, byom, society, e, workdir, genesis))
        per_source_trails(ctx, ev, which)
        honesty_labels(ev, RECORDING,
                       f"the agent half was driven by a real {which} "
                       "session over the real byom-mcp; the model call "
                       "itself used the recording transport, so this mode "
                       "makes no provider call")
        ev.step(f"{ctx['agent'].sessions} real {which} sessions drove the "
                "agent's own steps (mandate, activity, wake, pledge "
                "seats, the model_egress ACT CHAIN and the delivery) — "
                "identical tool schemas and zero server-side changes "
                "versus --scripted; every identifier above was recovered "
                "from byomd's OWN store and ledger",
                harness=which, sessions=ctx["agent"].sessions)
        ev.step("what passed each step, per session: the tool INVOCATION "
                "on the byom MCP wire (recorded by the relay, server-side, "
                "identically for both harnesses — not the CLI's prose) AND "
                "byomd's own effect event, minted after that session "
                "started. A session's `DONE …` line is never the signal: a "
                "session that makes no call fails its own step, and a call "
                "byom refuses fails it with the problem body",
                sessions=ctx["agent"].sessions,
                per_session_evidence=["session-NN-<tool>.txt",
                                      "session-NN-<tool>.byom-wire.jsonl"],
                effect_events=sorted(set(HARNESS_EFFECT.values())))
        rows = plan_coverage(ev, {
            f"harness-{which}": f"a REAL {which} CLI session per agent step, "
                                f"held to its MCP wire and byomd's ledger"})
        print_coverage(rows, f"{test_id}: plan §8 I1 coverage (this mode):")
        print(f"{test_id}: PASS ({ctx['agent'].sessions} real {which} "
              f"sessions, {ev.n} steps; evidence "
              f"{ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()
        cleanup_live()
        shutil.rmtree(workdir, ignore_errors=True)


# -------------------------------------------------------- crash matrix ----

# The NEW commit points I1 introduces, one cell each. Every cell arms a
# real fault, fires it, and then asserts from BOTH daemons' own records:
# exactly-once effects, never a second MandateUse, `ambiguous` where the
# outcome is genuinely unknown, and byte-identical replays.
CRASH_CELLS = [
    "byom/execution_permit_consume@before_witness",
    "byom/execution_permit_consume@after_finalize",
    "kovee/model_effect@after_prepare",
    # Named for what it IS (R3-I03): the abort point is statically before
    # `dispatch_bytes`, so the attempt row is committed `dispatching` and
    # nothing has been transmitted.
    "kovee/model_effect@after_dispatch_record_before_wire",
    # The genuine post-write uncertainty: the send is RECORDED and the
    # outcome is unknown.
    "kovee/model_effect@post_write_uncertain",
    "byom/usage_report@before_witness",
    "byom/usage_report@after_finalize",
    "kovee/endeavor_promotion_start@after_commit",
]


def cell_slug(cell: str) -> str:
    """One evidence directory per cell (R3-I04). Seven cells used to write
    `driver-01-complete.json` into one directory, each overwriting the
    last, so only the final kill left raw evidence behind."""
    return re.sub(r"[^a-z0-9]+", "-", cell.lower()).strip("-")


def crash_cell_setup(cell: str, ev: Evidence, tag: str) -> dict:
    ev.namespace(cell_slug(cell))
    ctx = governed_setup(ev, tag, {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
    ev.step(f"{cell}: the governed loop is set up to the committed Pledge "
            "on both live daemons",
            society=ctx["society"], episode=ctx["episode"],
            evidence_namespace=cell_slug(cell))
    return ctx


def armed_broker_state(ctx: dict, tag: str, key: str) -> tuple:
    """Everything the broker needs, up to an AUTHORIZED act: the worker
    attempt, the committed disclosure and byom's one-shot permit."""
    kovee, byom, driver, agent = (ctx["kovee"], ctx["byom"], ctx["driver"],
                                  ctx["agent"])
    call_args, _ = worker_call(kovee, ctx, RECORDING, "Say OK.", key)
    driver.ok("seed-bindings", {})
    staged = driver.ok("stage", call_args)
    act = prepare_act(byom, agent, ctx, staged, tag, key)
    _, finalized = authorize_act(byom, act, tag, key)
    authorization = act_authorization(
        byom, act, finalized["result"]["revision"])
    return call_args, act, authorization


def byom_permit_cell(cell: str, phase: str, ev: Evidence) -> None:
    """byomd dies inside `execution_permit_consume` — before the witness
    CAS (nothing may exist) or after the finalize (committed, reply lost).
    Either way: exactly one receipt, exactly one MandateUse, and kovee
    reaches a completed effect on the retry."""
    tag = f"i1c{len(cell) % 7}p"
    ctx = crash_cell_setup(cell, ev, tag)
    try:
        byom, driver = ctx["byom"], ctx["driver"]
        call_args, act, authorization = armed_broker_state(ctx, tag, "c")
        key = act["stable_execution_key"]
        byom.restart({"BYOMD_ABORT":
                      f"{phase}:execution_permit_consume"})
        armed_pid = byom.pid()
        refused = driver.problem("complete", {
            **call_args, **RECORDING["args"],
            "authorization": authorization})
        # R3-I03: the kill is now REQUIRED, not assumed — byomd died of the
        # armed abort signal and a replacement process answers afterwards.
        killed = byom.died_and_was_replaced(armed_pid, SIGABRT)
        need(driver.durable_sends() == 0,
             f"a permit consumption that never returned cannot have sent "
             f"anything: {driver.last_sends}")
        # What survived the kill, read from byom's OWN store.
        receipts = byom.count("SELECT COUNT(*) FROM"
                              " execution_consumption_receipts")
        uses = byom.count("SELECT COUNT(*) FROM mandate_uses")
        if phase == "before_witness":
            need(receipts == 0 and uses == 0,
                 f"{cell}: a pre-witness crash leaves NO receipt and NO "
                 f"MandateUse: {receipts}/{uses}")
        else:
            need(receipts == 1 and uses == 1,
                 f"{cell}: the committed transaction survives exactly "
                 f"once: {receipts}/{uses}")
        effect = driver.ok("effect-show", {"execution_key": key})
        need(effect["effect"]["state"] == "prepared",
             f"{cell}: kovee's effect is still prepared — nothing was "
             f"dispatched: {effect}")
        need(effect["attempts"] == [],
             f"{cell}: no attempt exists: {effect}")
        # The exact retry, on a daemon with no fault armed.
        completion = driver.ok("complete", {
            **call_args, **RECORDING["args"],
            "authorization": authorization})
        need(completion["state"] == "completed", f"{cell}: {completion}")
        need(byom.count("SELECT COUNT(*) FROM"
                        " execution_consumption_receipts") == 1
             and byom.count("SELECT COUNT(*) FROM mandate_uses") == 1,
             f"{cell}: exactly one receipt and one MandateUse after the "
             f"retry")
        after = driver.ok("effect-show", {"execution_key": key})
        need(len(after["attempts"]) == 1,
             f"{cell}: exactly one dispatch attempt: {after}")
        need(byom.ledger()["conserves"], f"{cell}: conservation")
        ev.step(f"{cell}: killed byomd inside the permit consumption "
                "(the armed process died of SIGABRT and a REPLACEMENT pid "
                "came up), restarted; the retry consumed the SAME one-shot "
                "permit and dispatched exactly once — one receipt, one "
                "MandateUse, one attempt, conservation intact",
                refused=refused.get("type"),
                kill=killed, durable_sends_during_crash=0,
                receipts_after_crash=receipts, uses_after_crash=uses,
                receipts_after_retry=1, mandate_uses_after_retry=1,
                attempts=1)
    finally:
        cleanup_live()


def kovee_effect_cell(cell: str, fault: str, ev: Evidence) -> None:
    """The broker's own write order, proven by a real process abort:
    `after_prepare` (the Effect is on disk, no permit consumed, nothing
    sent) and `after_dispatch_record_before_wire` (the attempt is committed
    `dispatching` and the process dies BEFORE the socket opens, so the
    outcome is unknown to kovee and must resolve AMBIGUOUS with retry
    frozen).

    R3-I03 named the second cell's honesty problem: it was called "after
    dispatch" while the abort is statically before `dispatch_bytes`, so
    nothing had been transmitted and the cell proved recovery of a
    write-order gap rather than of a real uncertainty. The name is now
    accurate, and the genuine post-write uncertainty point is its own cell
    (`kovee_uncertain_cell`)."""
    tag = f"i1c{len(fault) % 7}e"
    ctx = crash_cell_setup(cell, ev, tag)
    try:
        byom, kovee, driver = ctx["byom"], ctx["kovee"], ctx["driver"]
        call_args, act, authorization = armed_broker_state(ctx, tag, "c")
        key = act["stable_execution_key"]
        _, code = driver.run("complete", {
            **call_args, **RECORDING["args"], "fault": fault,
            "authorization": authorization}, expect_ok=False)
        # R3-I03: the ARMED SIGNAL is required, not merely a non-zero exit
        # (a refusal, a bad argument or a missing binary all exit non-zero
        # and none of them is the abort this cell claims to have caused),
        # and an aborted process leaves NO reply line and NO send counter.
        need(code == -SIGABRT,
             f"{cell}: the armed broker must die of SIGABRT, exit was "
             f"{code}")
        need(driver.last_stdout == "",
             f"{cell}: an aborted process answers nothing; it printed "
             f"{driver.last_stdout!r}")
        need(driver.no_send_counter(),
             f"{cell}: an aborted process cannot have reported a send "
             f"count: {driver.last_sends}")
        effect = driver.ok("effect-show", {"execution_key": key})
        receipts = byom.count("SELECT COUNT(*) FROM"
                              " execution_consumption_receipts")
        uses = byom.count("SELECT COUNT(*) FROM mandate_uses")
        if fault == "after_prepare":
            need(effect["effect"]["state"] == "prepared",
                 f"{cell}: the prepared Effect is on disk: {effect}")
            need(effect["attempts"] == [] and effect["consumptions"] == [],
                 f"{cell}: no permit was consumed and nothing was sent: "
                 f"{effect}")
            need(receipts == 0 and uses == 0,
                 f"{cell}: byom saw no consumption: {receipts}/{uses}")
            completion = driver.ok("complete", {
                **call_args, **RECORDING["args"],
                "authorization": authorization})
            need(completion["state"] == "completed",
                 f"{cell}: the re-run completes: {completion}")
            after = driver.ok("effect-show", {"execution_key": key})
            need(after["effect"]["effect_id"]
                 == effect["effect"]["effect_id"],
                 f"{cell}: the SAME effect, found by byom's stable "
                 f"execution key — never a second one: {after}")
            need(len(after["attempts"]) == 1
                 and byom.count("SELECT COUNT(*) FROM mandate_uses") == 1,
                 f"{cell}: exactly one attempt and one MandateUse: "
                 f"{after}")
            ev.step(f"{cell}: aborted the broker right after the Effect "
                    "was committed `prepared` — no permit consumed, no "
                    "byte sent, no receipt; the re-run found the SAME "
                    "effect by byom's stable execution key and dispatched "
                    "exactly once",
                    effect=effect["effect"]["effect_id"],
                    state_after_crash="prepared",
                    byom_receipts_after_crash=0,
                    attempts_after_rerun=1, mandate_uses=1)
            return
        # after_dispatch_record_before_wire: the attempt row is committed
        # `dispatching` and the process dies BEFORE `dispatch_bytes`, so
        # NOTHING was transmitted and kovee cannot know that. The recovery
        # obligation is identical to a real post-write loss, which is what
        # the cell proves; the genuine post-write case is its own cell.
        need(len(effect["attempts"]) == 1
             and effect["attempts"][0]["state"] == "dispatching",
             f"{cell}: the attempt is committed dispatching BEFORE the "
             f"socket opens: {effect}")
        need(receipts == 1 and uses == 1,
             f"{cell}: the permit was consumed exactly once: "
             f"{receipts}/{uses}")
        # koveed's startup sweep is the only honest resolution.
        kovee.restart({"ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
        swept = driver.ok("effect-show", {"execution_key": key})
        need(swept["attempts"][0]["state"] == "ambiguous"
             and swept["attempts"][0]["retry_frozen"] is True,
             f"{cell}: an unknown outcome resolves AMBIGUOUS with retry "
             f"frozen — never retried, never written off: {swept}")
        spent = driver.problem("complete", {
            **call_args, **RECORDING["args"],
            "authorization": authorization})
        need("spent" in str(spent.get("detail") or ""),
             f"{cell}: the spent one-shot permit refuses a second "
             f"dispatch: {spent}")
        need(byom.count("SELECT COUNT(*) FROM mandate_uses") == 1,
             f"{cell}: never a second MandateUse")
        after = driver.ok("effect-show", {"execution_key": key})
        need(len(after["attempts"]) == 1,
             f"{cell}: never a second attempt: {after}")
        ev.step(f"{cell}: SIGABRT'd the broker right after the attempt was "
                "committed `dispatching` and BEFORE the socket opened "
                "(the fault point is statically before dispatch_bytes, and "
                "the cell name now says so) — kovee cannot know that, so "
                "koveed's startup sweep resolves it AMBIGUOUS with retry "
                "frozen; the spent one-shot permit then refuses a second "
                "dispatch and byom never sees a second MandateUse",
                state_after_crash="dispatching",
                state_after_sweep="ambiguous", retry_frozen=True,
                bytes_transmitted="none (abort precedes dispatch_bytes)",
                byom_receipts=1, byom_mandate_uses=1, attempts=1)
    finally:
        cleanup_live()


def kovee_uncertain_cell(cell: str, ev: Evidence) -> None:
    """The GENUINE post-write uncertainty point (R3-I03).

    The two broker abort cells both fire before `dispatch_bytes`, so no
    request ever left. This one drives kovee's own `RecordingTransport::
    uncertain`: the transport RECORDS the send — the external counter reads
    1 — and then reports `TransportError::Uncertain`, exactly the case
    where the provider may have received and billed the request. That is
    the state the effect must resolve `ambiguous` with retry frozen, and
    where byom's one-shot permit must already be spent."""
    tag = "i1cunc"
    ctx = crash_cell_setup(cell, ev, tag)
    try:
        byom, kovee, driver = ctx["byom"], ctx["kovee"], ctx["driver"]
        call_args, act, authorization = armed_broker_state(ctx, tag, "c")
        key = act["stable_execution_key"]
        outcome = driver.ok("complete", {
            **call_args, "transport": "recording_uncertain",
            "uncertain_reason": "connection reset after the request flushed",
            "authorization": authorization})
        need(outcome["state"] == "ambiguous",
             f"{cell}: an uncertain send is AMBIGUOUS, never failed: "
             f"{outcome}")
        need(outcome["retry_frozen"] is True,
             f"{cell}: an ambiguous effect freezes retry: {outcome}")
        # The bytes DID leave, and the external counter says so — this is
        # what separates this cell from the two aborts.
        need(driver.durable_sends() == 1,
             f"{cell}: the request was transmitted exactly once: "
             f"{driver.last_sends}")
        effect = driver.ok("effect-show", {"execution_key": key})
        need(effect["effect"]["state"] == "ambiguous"
             and effect["attempts"][0]["state"] == "ambiguous"
             and effect["attempts"][0]["retry_frozen"] is True,
             f"{cell}: kovee's own rows hold the ambiguity: {effect}")
        need(effect["usage_reports"] == [],
             f"{cell}: no usage may be metered for an outcome nobody "
             f"observed: {effect}")
        need(byom.count("SELECT COUNT(*) FROM usage_settlements") == 0,
             f"{cell}: and nothing is settled")
        need(byom.count("SELECT COUNT(*) FROM"
                        " execution_consumption_receipts") == 1
             and byom.count("SELECT COUNT(*) FROM mandate_uses") == 1,
             f"{cell}: the permit was consumed exactly once, before the "
             f"wire")
        # A restart cannot turn an ambiguous outcome into either answer.
        old = kovee.pid()
        kovee.restart({"ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
        need(kovee.pid() != old, f"{cell}: koveed was not restarted")
        swept = driver.ok("effect-show", {"execution_key": key})
        need(swept["attempts"][0]["state"] == "ambiguous"
             and swept["attempts"][0]["retry_frozen"] is True,
             f"{cell}: the sweep never resolves an ambiguity it cannot "
             f"observe: {swept}")
        spent = driver.problem("complete", {
            **call_args, **RECORDING["args"],
            "authorization": authorization})
        need("spent" in str(spent.get("detail") or ""),
             f"{cell}: the spent permit refuses a retry of an ambiguous "
             f"effect: {spent}")
        need(driver.durable_sends() == 0,
             f"{cell}: and that refusal sent nothing: {driver.last_sends}")
        led = byom.ledger()
        need(led["conserves"], f"{cell}: conservation: {led}")
        ev.step(f"{cell}: a GENUINE post-write uncertainty — kovee's "
                "transport recorded the send (external counter = 1) and "
                "then reported an uncertain outcome: the effect and its "
                "attempt are AMBIGUOUS with retry frozen, no usage is "
                "metered and nothing is settled, byom's one-shot permit is "
                "already spent (one receipt, one MandateUse), a koveed "
                "restart does not resolve what nobody observed, and a "
                "retry is refused as spent with zero further sends",
                durable_sends=1, effect_state="ambiguous",
                retry_frozen=True, kovee_usage_reports=0,
                byom_settlements=0, byom_receipts=1, byom_mandate_uses=1,
                retry_refused_as=spent.get("type"), ledger=led)
    finally:
        cleanup_live()


def byom_usage_cell(cell: str, phase: str, ev: Evidence) -> None:
    """byomd dies inside `usage_report`. `before_witness` is armed during
    the BROKER's metering: the dispatch stands, the settlement does not,
    and kovee never claims usage it could not report. `after_finalize` is
    armed during the Episode's own metered settlement: byom committed and
    lost the reply, so the exact retry must replay — SettleOnce."""
    tag = f"i1c{len(phase) % 7}u"
    ctx = crash_cell_setup(cell, ev, tag)
    try:
        byom, driver = ctx["byom"], ctx["driver"]
        if phase == "before_witness":
            call_args, act, authorization = armed_broker_state(ctx, tag, "c")
            key = act["stable_execution_key"]
            byom.restart({"BYOMD_ABORT": f"{phase}:usage_report"})
            armed_pid = byom.pid()
            driver.problem("complete", {
                **call_args, **RECORDING["args"],
                "authorization": authorization})
            killed = byom.died_and_was_replaced(armed_pid, SIGABRT)
            need(driver.durable_sends() == 1,
                 f"{cell}: the dispatch itself DID happen — the metering "
                 f"report is what died: {driver.last_sends}")
            need(byom.count("SELECT COUNT(*) FROM usage_settlements") == 0,
                 f"{cell}: nothing settled")
            need(byom.count("SELECT COUNT(*) FROM usage_reports") == 0,
                 f"{cell}: no usage report survived the abort")
            need(byom.count("SELECT COUNT(*) FROM mandate_uses") == 1,
                 f"{cell}: the permit was consumed exactly once")
            effect = driver.ok("effect-show", {"execution_key": key})
            need(effect["usage_reports"] == [],
                 f"{cell}: kovee claims no metering it could not report: "
                 f"{effect}")
            need(len(effect["attempts"]) == 1,
                 f"{cell}: exactly one dispatch attempt: {effect}")
            led = byom.ledger()
            need(led["conserves"] and led["committed"]
                 == [r["amount"] for r in byom.reservations(
                     state="committed")
                     if r["holder_kind"] == "act_intent"][0],
                 f"{cell}: an unsettled report moves no ledger units: "
                 f"{led}")
            ev.step(f"{cell}: killed byomd inside the broker's metering "
                    "report (SIGABRT, replacement pid up) — the dispatch "
                    "stands and the external counter proves it (1 send), "
                    "NOTHING settled, the ledger moved no unit beyond the "
                    "act's own reservation, and kovee claims no metering "
                    "it could not report",
                    kill=killed, durable_sends=1,
                    byom_settlements=0, byom_usage_reports=0,
                    kovee_usage_reports=0, mandate_uses=1, ledger=led)
            return
        # after_finalize, on the Episode's own metered settlement.
        charge = 12
        byom.restart({"BYOMD_ABORT": f"{phase}:usage_report"})
        armed_pid = byom.pid()
        driver.problem("episode-settle", {
            "stable_binding_key": ctx["bound"]["stable_binding_key"],
            "charge": charge})
        killed = byom.died_and_was_replaced(armed_pid, SIGABRT)
        settled = byom.rows("SELECT stable_settlement_key, status,"
                            " charged_quantities FROM usage_settlements")
        need(len(settled) == 1,
             f"{cell}: the committed settlement survives exactly once: "
             f"{settled}")
        first = driver.ok("episode-settle", {
            "stable_binding_key": ctx["bound"]["stable_binding_key"],
            "charge": charge})
        need(byom.count("SELECT COUNT(*) FROM usage_settlements") == 1,
             f"{cell}: the retry replays the stored settlement, it never "
             f"settles twice")
        again = driver.ok("episode-settle", {
            "stable_binding_key": ctx["bound"]["stable_binding_key"],
            "charge": charge})
        need(first["usage_report"] == again["usage_report"],
             f"{cell}: replays are byte-identical: {first} vs {again}")
        led = byom.ledger()
        need(led["conserves"], f"{cell}: conservation: {led}")
        need(led["committed"] == charge,
             f"{cell}: exactly one charge of {charge}: {led}")
        ev.step(f"{cell}: killed byomd after the settlement committed but "
                "before the reply (SIGABRT, replacement pid up) — the "
                "retry REPLAYS the stored settlement (SettleOnce), replays "
                f"are byte-identical, and exactly {charge} units are "
                "committed once",
                kill=killed, byom_settlements=1, charged=charge, ledger=led,
                replay_byte_identical=True)
    finally:
        cleanup_live()


def kovee_formation_cell(cell: str, phase: str, ev: Evidence) -> None:
    """koveed dies at a formation saga commit point: at most ONE Endeavor
    exists on byom's side, and the slot never releases with nothing
    formed."""
    tag = "i1cf"
    point = "endeavor_promotion_start#result"
    ctx = None
    ev.namespace(cell_slug(cell))
    try:
        ctx = governed_setup(ev, tag,
                             {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY},
                             stop_after="activation")
        kovee, byom = ctx["kovee"], ctx["byom"]
        _, _, prepared = formation_prepare(kovee, byom, ctx, tag)
        kovee.restart({"KOVEED_ABORT": f"{phase}:{point}",
                       "ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
        armed_pid = kovee.pid()
        start = kv("endeavor_promotion_start", None, "idem-i1c-start",
                   {"formation_id": prepared,
                    "authentication_observation_ref": "authobs-crash-1"})
        first = kovee.call_raw(start)
        # R3-I03: this cell used to pass whether or not koveed died — a
        # `first` that ANSWERED meant the fault never fired and the rest of
        # the cell then verified an ordinary success. The kill is now
        # required: no reply line, death by the armed abort signal, and a
        # replacement process afterwards.
        need(first is None,
             f"{cell}: the armed commit point must kill koveed before it "
             f"can answer; it replied {first!r}")
        killed = kovee.died_and_was_replaced(
            armed_pid, SIGABRT, {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
        formed = byom.count("SELECT COUNT(*) FROM endeavors")
        need(formed <= 1, f"{cell}: {formed} Endeavors formed")
        # The exact retry, on a daemon with no fault armed.
        retried = kovee.call_raw(start)
        need(retried is not None, f"{cell}: the retry never answered")
        need(byom.count("SELECT COUNT(*) FROM endeavors") <= 1,
             f"{cell}: still at most one Endeavor after the retry")
        view = kovee.expect_ok(kv("endeavor_promotion_show", None, None,
                                  {"formation_id": prepared}))["result"]
        if byom.count("SELECT COUNT(*) FROM endeavors") == 1:
            if view["state"] != "linked":
                kovee.call_raw(start)
                kovee.call_raw(kv("endeavor_promotion_reconcile", None,
                                  "idem-i1c-rec",
                                  {"formation_id": prepared}))
                kovee.call_raw(start)
                view = kovee.expect_ok(kv(
                    "endeavor_promotion_show", None, None,
                    {"formation_id": prepared}))["result"]
            need(view["state"] == "linked"
                 and view["slot"]["state"] == "released",
                 f"{cell}: a formed Endeavor must reach linked: {view}")
        else:
            need(view["slot"]["state"] != "released",
                 f"{cell}: the slot released with nothing formed: {view}")
        a = kovee.call_raw(start)
        b = kovee.call_raw(start)
        need(a == b, f"{cell}: replays must be byte-identical")
        need(byom.count("SELECT COUNT(*) FROM endeavors") == 1,
             f"{cell}: exactly one Endeavor at the end")
        ev.step(f"{cell}: killed koveed at the formation saga's {phase} "
                "commit point — the armed call got NO reply, the process "
                "died of SIGABRT and a REPLACEMENT pid came up; at most "
                "one Endeavor ever exists on byom's side, the slot never "
                "releases with nothing formed, and the retry reaches "
                "`linked` with byte-identical replays",
                kill=killed, formed_after_crash=formed, formed_at_end=1,
                state=view["state"], slot=view["slot"]["state"])
    finally:
        cleanup_live()


def mode_crash_matrix() -> int:
    ev = Evidence("i1-crash")
    print("i1-crash: kill both daemons and the broker chain at the NEW I1 "
          "commit points (BYOMD_ABORT / KOVEED_ABORT / the broker's own "
          "Fault hooks)")
    try:
        pinned = assert_pinned(ev)
        ev.step("the revisions this matrix is running, ASSERTED (R3-I02)",
                byom=pinned["byom"], kovee=pinned["kovee"],
                driver_built_against=pinned["driver_built_against"])
        byom_permit_cell(CRASH_CELLS[0], "before_witness", ev)
        byom_permit_cell(CRASH_CELLS[1], "after_finalize", ev)
        kovee_effect_cell(CRASH_CELLS[2], "after_prepare", ev)
        kovee_effect_cell(CRASH_CELLS[3], "after_dispatch_record", ev)
        kovee_uncertain_cell(CRASH_CELLS[4], ev)
        byom_usage_cell(CRASH_CELLS[5], "before_witness", ev)
        byom_usage_cell(CRASH_CELLS[6], "after_finalize", ev)
        kovee_formation_cell(CRASH_CELLS[7], "after_commit", ev)
        print(f"i1-crash: PASS ({len(CRASH_CELLS)} cells; evidence "
              f"{ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()
        cleanup_live()


# ---------------------------------------------------------------- main ----

def mode_coverage_rows(test_id: str) -> list:
    """One mode's own coverage statement, read back from the evidence it
    just wrote. `--all-checks` unions these, so the whole-gate claim rests
    on artifacts rather than on a summary line."""
    path = EVIDENCE / test_id / COVERAGE_BLOB
    if not path.exists():
        return []
    return json.loads(path.read_text(encoding="utf-8"))


def mode_all_checks() -> int:
    """Every check this gate has, deterministic and env-gated alike, and
    then the WHOLE gate held to the plan-§8 I1 item list (R3-I01).

    A SKIP IS NOT A PASS. `--all-checks` used to run both real-harness
    modes, let each answer exit 2, print `SKIP`, and then return 0 — so the
    suite could report green having run neither harness, and two of the
    plan's three execution paths would rest on nothing but their
    deterministic stand-in. It now exits 2 in that case: an honest
    INCOMPLETE, distinguishable from both the pass and the failure, and it
    says which item is left standing on a simulation.

    The coverage statement at the end is the union of every mode's own
    statement, each of which was checked against that mode's own evidence.
    An item nobody exercised fails the gate; an item only a SIMULATED proof
    covers is named, with what stands in, and is not counted as covered."""
    results, coverage = [], []
    for name, test_id, mode in (
            ("scripted", "i1-flow-scripted", mode_scripted),
            ("crash-matrix", "i1-crash", mode_crash_matrix),
            ("verify-trails", "i1-trails", mode_verify_trails),
            ("attached-claude", "i1-attached-claude",
             lambda: mode_attached_path("claude")),
            ("attached-codex", "i1-attached-codex",
             lambda: mode_attached_path("codex"))):
        code = mode()
        results.append((name, code, "PASS" if code == 0 else "FAIL"))
        if code != 0:
            print(f"i1: all checks FAILED at {name} (exit {code})")
            return code
        coverage.append(mode_coverage_rows(test_id))
    incomplete = []
    for which in ("claude", "codex"):
        code = mode_harness(which)
        if code == 2:
            results.append((f"real-harness-{which}", 2,
                            "SKIP — NOT A PASS (needs I1_REAL_HARNESS=1 and "
                            f"the {which} CLI on PATH)"))
            incomplete.append(f"real-harness-{which}")
        elif code == 0:
            results.append((f"real-harness-{which}", 0, "PASS (real "
                            f"{which} session)"))
            coverage.append(mode_coverage_rows(f"i1-flow-{which}"))
        else:
            print(f"i1: all checks FAILED at real-harness-{which}")
            return code
    rows = merge_coverage(coverage)
    (EVIDENCE / "all-checks-coverage.json").write_text(
        json.dumps(rows, indent=1), encoding="utf-8")
    print("\ni1 all-checks summary:")
    for name, code, state in results:
        print(f"  {name:<20} exit {code}  {state}")
    print_coverage(rows, "i1 all-checks: plan §8 I1 coverage, over EVERY "
                         "mode that ran:")
    uncovered = [r["plan_8_I1_item"] for r in rows
                 if r["status"].upper().startswith("NOT")]
    if uncovered:
        print("i1: all checks FAILED — no mode of this run exercised: "
              + "; ".join(uncovered))
        return 1
    if incomplete:
        print(f"i1: all checks INCOMPLETE (exit 2) — {', '.join(incomplete)} "
              "did not run, and a SKIP is not a PASS. The plan-§8 execution "
              "paths those modes own are covered here only by their "
              "DETERMINISTIC stand-in, which is reported as SIMULATED "
              "above. Set I1_REAL_HARNESS=1 with both CLIs on PATH for a "
              "green --all-checks.")
        return 2
    print("i1: all checks PASS — every plan-§8 I1 item is covered by a mode "
          "that ran, each claim backed by that mode's own evidence, and "
          "everything still standing in is named SIMULATED above. "
          "--real-model is separate (it spends money).")
    return 0


def main(argv: list) -> int:
    args = argv[1:]
    try:
        if args == ["--scripted"]:
            return mode_scripted()
        if args == ["--crash-matrix"]:
            return mode_crash_matrix()
        if args == ["--verify-trails"]:
            return mode_verify_trails()
        if args == ["--real-model"]:
            return mode_real_model()
        if len(args) == 2 and args[0] == "--attached-path" \
                and args[1] in ("claude", "codex"):
            return mode_attached_path(args[1])
        if len(args) == 2 and args[0] == "--harness" \
                and args[1] in ("claude", "codex"):
            return mode_harness(args[1])
        if args == ["--all-checks"]:
            return mode_all_checks()
        if len(args) == 3 and args[0] == "--_agent-call":
            return mode_agent_call(args[1], args[2])
        if len(args) >= 3 and args[0] == "--_mcp-wire":
            return mode_mcp_wire(args[1], args[2:])
    except Fail as e:
        print(f"FAIL  {e}")
        return 1
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
