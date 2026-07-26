#!/usr/bin/env python3
"""I0 society-of-two tracer (the integration gate; plan/sheets/I0.md).

Both stacks live side by side — byomd (this repo) and koveed
(../kovee) — each with isolated data/runtime dirs. The byom Society runs
the complete attached-only governed flow with the agent seat driven
through the REAL byom-mcp binary over scripted MCP JSON-RPC (the exact
stdio surface Claude Code / Codex speak); kovee runs standalone
(init -> space -> question -> a contribution round-trip through the real
kovee-mcp). Trails are asserted PER SOURCE — byom events_read over byom
records, kovee events over kovee records — never merged.

    python3 run.py --scripted        # i0-flow-scripted (gates CI)
    python3 run.py --crash-matrix    # i0-crash (BYOMD_ABORT / KOVEED_ABORT)
    python3 run.py --verify-trails   # i0-trails (per-source attribution)
    python3 run.py --harness claude  # i0-flow-claude (env-gated: I0_REAL_HARNESS=1)
    python3 run.py --harness codex   # i0-flow-codex (env-gated)
    python3 run.py --all-checks      # scripted + crash + trails + the cargo
                                     # suites wired as i0-negative /
                                     # i0-privacy / i0-classification

The harness modes run the SAME flow as --scripted with the agent's own
steps performed by a real Claude Code / Codex session over the real
byom-mcp and kovee-mcp servers (successive sessions, because a governed
flow interleaves with the human's steps). Every claim they make is
verified afterwards from byomd's and koveed's own event ledgers — never
from what the harness says.

Who speaks on which channel (the attribution the trails verify):
  - governance ops (genesis, offer, admissions, mandate seat+issue):
    the direct human channel — the governance socket under the
    operator's uid (actor `governance:sovereign`);
  - candidate ops (membership_accept): the byom-mcp CANDIDATE profile
    as a subprocess, the offer's channel credential via
    BYOM_CANDIDATE_TOKEN_FILE — a keyless channel binding, not a
    bearer token: the holder claims a peer-bound proof key over the
    socket and every call carries a fresh proof bound to the
    connecting process (actor `candidate:<channel>`); the channel
    closes at admission;
  - agent participant ops (mandate_prepare, activity_open,
    wake_intent_submit, pledge_propose, pledgor pledge_position,
    pledge_finalize, delivery_submit): the byom-mcp PARTICIPANT profile
    with the minted participant credential (actor
    `participant:part-agent-1`);
  - human participant ops (endeavor propose/position/finalize,
    call_open, beneficiary pledge_position, review_record): the direct
    human channel — the participant socket with no channel credential,
    which byomd resolves to the sovereign (actor
    `participant:<sovereign>`). byom-cli does not yet expose
    pledge/endeavor/review verbs, so the human seat rides its socket
    directly; the CLI is exercised for every verb it does have
    (`society show`, `events`).
  - kovee: the kovee CLI (init, space create, question contribution)
    and the real kovee-mcp (the scripted contribution round-trip).

Evidence lands in evidence/<test-id>/ next to this file.
Exit codes: 0 green, 1 failure, 2 honest skip (ungated harness mode).
"""

import hashlib
import hmac
import json
import os
import re
import shutil
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

AGENT = "part-agent-1"
GOV_ACTOR = "governance:sovereign"
KOVEE_ACTOR = "prin-owner"  # the personal-profile owner principal
FAR_FUTURE = "2030-01-01T00:00:00Z"


class Fail(Exception):
    """One failed assertion; the runner reports and exits 1."""


def need(cond, detail):
    if not cond:
        raise Fail(detail)


# ------------------------------------------------------------ binaries ----

_target_cache: dict[str, str] = {}


def _target_dir(repo: Path) -> Path:
    key = str(repo)
    if key not in _target_cache:
        meta = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=repo))
        _target_cache[key] = meta["target_directory"]
    return Path(_target_cache[key])


def _binary(repo: Path, package: str, name: str) -> str:
    path = _target_dir(repo) / "debug" / name
    if not path.exists():
        subprocess.check_call(
            ["cargo", "build", "-q", "-p", package,
             "--manifest-path", str(repo / "Cargo.toml")])
    need(path.exists(), f"binary missing after build: {path}")
    return str(path)


def byomd_bin():
    return _binary(REPO, "byomd", "byomd")


def byom_cli_bin():
    return _binary(REPO, "byom-cli", "byom")


def byom_mcp_bin():
    return _binary(REPO, "byom-mcp", "byom-mcp")


def koveed_bin():
    return _binary(KOVEE_ROOT, "koveed", "koveed")


def kovee_cli_bin():
    return _binary(KOVEE_ROOT, "kovee-cli", "kovee")


def kovee_mcp_bin():
    return _binary(KOVEE_ROOT, "kovee-mcp", "kovee-mcp")


# ------------------------------------------------------------ evidence ----

class Evidence:
    """Per-test-id evidence: numbered step lines on stdout, a
    steps.jsonl transcript, and named blobs, under evidence/<test-id>/."""

    def __init__(self, test_id: str):
        self.dir = EVIDENCE / test_id
        shutil.rmtree(self.dir, ignore_errors=True)
        self.dir.mkdir(parents=True)
        self.test_id = test_id
        self.n = 0
        self._steps = (self.dir / "steps.jsonl").open("w", encoding="utf-8")

    def step(self, title: str, **detail):
        self.n += 1
        row = {"step": self.n, "title": title, **detail}
        self._steps.write(json.dumps(row) + "\n")
        self._steps.flush()
        print(f"  ok {self.n:02d}  {title}")

    def blob(self, name: str, text: str):
        (self.dir / name).write_text(text, encoding="utf-8")

    def close(self):
        self._steps.close()


# ----------------------------------------------- channel proofs (BY-C1) ----

# The candidate/participant credential is NOT a bearer token, and since
# the R1 confirmation it carries NO KEY MATERIAL AT ALL. What byomd mints
# in `<data-dir>/channels/*.token` is only the public binding
#
#     bpk1.<hex JSON {channel_id, audience, scope_ref, binding_ref,
#                     fence_epoch}>
#
# so a copy of that file mints nothing. A client CLAIMS its channel once
# over the surface socket
#
#     bpb1.<channel_id>            -> {"outcome":"ok",
#                                      "result":{"proof_key":"<hex>"}}
#
# and byomd answers with a proof key bound to THAT connection's
# kernel-observed peer, refusing any other live process. Every call then
# carries a FRESH per-call proof
#
#     bpx1.<channel_id>.<nonce>.<issued_at>.<mac>
#
# MAC'd under that peer-bound key over the exact (audience, channel,
# scope, operation, binding, fence, PEER pid + kernel start time, nonce,
# issued_at) — crates/byomd/src/channel.rs. This is the client half of
# that construction, ported so the scenario's direct-socket calls speak
# exactly what byom-mcp and byom-cli speak.

CHANNEL_PROOF_TAG = "bpp-channel-proof-v0"


def jcs(value) -> bytes:
    """JCS (RFC 8785) for the shapes used here: objects with ASCII keys,
    string and integer values — byte-identical to bpp-core's `jcs`."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False).encode()


def tagged_canonical(tag: str, obj: dict) -> bytes:
    return jcs({**obj, "$domain": tag})


def peer_process_start(pid: int) -> int:
    """`/proc/<pid>/stat` field 22 — the kernel start time byomd reads
    through SO_PEERCRED to pin the exact process, not a recycled pid."""
    try:
        stat = Path(f"/proc/{pid}/stat").read_text(encoding="utf-8")
    except OSError:
        return 0
    fields = stat.rsplit(")", 1)[-1].split()
    try:
        return int(fields[19])
    except (IndexError, ValueError):
        return 0


def parse_credential(line: str) -> dict:
    body = line.strip()
    need(body.startswith("bpk1."),
         f"not a byom channel credential: {body[:12]!r}")
    return json.loads(bytes.fromhex(body[len("bpk1."):]))


def mint_proof(credential_line: str, key: bytes, operation: str) -> str:
    """One sender-constrained proof for exactly this call, under the
    peer-bound key this process claimed."""
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
        s.settimeout(30)
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


class ByomDaemon:
    SURFACES = ("governance", "candidate", "participant", "projection")

    def __init__(self, tag: str, env: dict | None = None):
        self.data_dir = Path(tempfile.mkdtemp(prefix=f"i0-byom-{tag}-data-"))
        self.run_dir = Path(tempfile.mkdtemp(prefix=f"i0-byom-{tag}-run-"))
        self.proc = None
        # Peer-bound proof keys this process claimed, per credential
        # (BY-C1): claimed once, kept across daemon restarts.
        self._claimed: dict[str, bytes] = {}
        self.start(env)

    def start(self, env: dict | None = None):
        full = {**os.environ, "BYOM_DATA_DIR": str(self.data_dir),
                "BYOM_RUNTIME_DIR": str(self.run_dir)}
        full.pop("BYOMD_ABORT", None)
        full.update(env or {})
        self.proc = subprocess.Popen(
            [byomd_bin()], env=full,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 15
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
        preamble = token if surface == "candidate" else token
        # candidate always takes a preamble line (possibly empty)
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
        reply = self.expect_ok("governance", {"version": "0.2", "op": "hello"})
        return reply["result"]["endpoint_incarnation"]

    def token_file(self, name: str) -> Path:
        return self.data_dir / "channels" / name

    def read_token(self, name: str) -> str:
        return self.token_file(name).read_text(encoding="utf-8").strip()

    def claim(self, name: str) -> bytes:
        """This PROCESS's peer-bound proof key for one channel: claimed
        once over the surface socket and kept (BY-C1). The credential
        file carries no key material, so there is nothing else to hold.
        """
        credential = self.read_token(name)
        cached = self._claimed.get(credential)
        if cached is not None:
            return cached
        cred = parse_credential(credential)
        surface = ("candidate" if cred["audience"] == "candidate"
                   else "participant")
        raw = _unix_call(self.run_dir / f"{surface}.sock",
                         f"bpb1.{cred['channel_id']}", None)
        need(raw is not None, f"channel claim got no reply for {name}")
        reply = json.loads(raw)
        need(reply.get("outcome") == "ok",
             f"channel claim refused for {name}: {raw}")
        key = bytes.fromhex(reply["result"]["proof_key"])
        self._claimed[credential] = key
        return key

    def proof(self, name: str, operation: str) -> str:
        """A fresh channel proof for one call, minted under this
        process's claimed key from the current credential file (re-read
        every time: a restart or a fence advance rewrites it)."""
        credential = self.read_token(name)
        return mint_proof(credential, self.claim(name), operation)

    def store_row(self, table: str, key_col: str, key: str) -> dict:
        """One row of byomd's OWN database, opened read-only beside the
        running daemon — the same inspection channel the Rust suites
        use. Only ever used to recover daemon-DERIVED values the driver
        must echo back (subject digests, seat refs, revisions); every
        ASSERTION in this scenario is made against the event ledger."""
        conn = sqlite3.connect(f"file:{self.data_dir / 'byom.db'}?mode=ro",
                               uri=True)
        try:
            conn.row_factory = sqlite3.Row
            row = conn.execute(
                f"SELECT * FROM {table} WHERE {key_col} = ?",  # noqa: S608
                (key,)).fetchone()
        finally:
            conn.close()
        need(row is not None, f"{table}: no row {key_col}={key}")
        return dict(row)

    def kill(self):
        if self.proc is not None:
            self.proc.kill()
            self.proc.wait()
            self.proc = None

    def wait_exit(self):
        if self.proc is not None:
            self.proc.wait()
            self.proc = None

    def restart(self):
        self.kill()
        self.start()

    def cleanup(self):
        self.kill()
        shutil.rmtree(self.data_dir, ignore_errors=True)
        shutil.rmtree(self.run_dir, ignore_errors=True)


class Koveed:
    def __init__(self, tag: str, env: dict | None = None):
        base = Path(tempfile.mkdtemp(prefix=f"i0-kovee-{tag}-"))
        self.data_dir = base / "data"
        self.run_dir = base / "run"
        self.base = base
        self.data_dir.mkdir()
        self.run_dir.mkdir()
        self.proc = None
        self.start(env)

    def start(self, env: dict | None = None):
        full = {**os.environ, "KOVEE_RUNTIME_DIR": str(self.run_dir)}
        full.pop("KOVEED_ABORT", None)
        full.update(env or {})
        self.proc = subprocess.Popen(
            [koveed_bin(), "--data-dir", str(self.data_dir)], env=full,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 15
        while True:
            reply = _unix_call(self.run_dir / "kovee.sock",
                               json.dumps({"version": "0.1", "op": "hello",
                                           "args": {
                                               "supported_versions": ["0.1"],
                                               "implementation": "i0-run",
                                               "implementation_version": "0",
                                               "requested_features": []}}),
                               None)
            if reply is not None:
                return
            need(time.time() < deadline, "koveed socket never came up")
            time.sleep(0.03)

    def call_raw(self, line: str) -> str | None:
        return _unix_call(self.run_dir / "kovee.sock", line, None)

    def call(self, request: dict) -> dict:
        raw = self.call_raw(json.dumps(request))
        need(raw is not None, f"koveed died on {request.get('op')}")
        return json.loads(raw)

    def expect_ok(self, request: dict) -> dict:
        reply = self.call(request)
        need(reply.get("outcome") == "ok",
             f"{request.get('op')}: {json.dumps(reply)}")
        return reply

    def kill(self):
        if self.proc is not None:
            self.proc.kill()
            self.proc.wait()
            self.proc = None

    def wait_exit(self):
        if self.proc is not None:
            self.proc.wait()
            self.proc = None

    def restart(self):
        self.kill()
        self.start()

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
    return {"class": "local_erasure_safe", "algorithm": "hmac-sha-256",
            "key_ref": f"test-key-{seed}", "value_hex": f"{seed:02x}" * 32}


def kv_mutation(op: str, project: str | None, key: str, args: dict) -> dict:
    cmd = {"version": "0.1", "op": op,
           "meta": {"request_id": f"req-{key}", "idempotency_key": key},
           "realm_id": "realm-personal"}
    if project is not None:
        cmd["project_id"] = project
    cmd["args"] = args
    return cmd


def kv_read(op: str, project: str | None, args: dict) -> dict:
    cmd = {"version": "0.1", "op": op, "realm_id": "realm-personal"}
    if project is not None:
        cmd["project_id"] = project
    cmd["args"] = args
    return cmd


# The kovee §10.3 branch-head fold (kovee-core/src/branch.rs, ported so
# the scripted MCP client derives heads exactly as any authorized
# reader would — never a privileged read).
def _tbd(domain: str, ref: str, data: bytes) -> str:
    def frame(b: bytes) -> bytes:
        return len(b).to_bytes(8, "big") + b
    buf = (frame(b"dev.kovee.typed-bytes-digest.v1") + frame(domain.encode())
           + frame(b"0") + frame(ref.encode()) + frame(data))
    return hashlib.sha256(buf).hexdigest()


KOVEE_BRANCH_REF = "https://kovee.example/kcp/v0/branch-head.v1"


def kovee_genesis_head(branch_id: str) -> str:
    return _tbd("branch-head", KOVEE_BRANCH_REF,
                f"genesis:{branch_id}".encode())


def kovee_next_head(prev: str, seq: int, content_digest: str) -> str:
    return _tbd("branch-head", KOVEE_BRANCH_REF,
                f"{prev}:{seq}:{content_digest}".encode())


# ---------------------------------------------------------- MCP client ----

class Mcp:
    """A scripted MCP stdio client driving a real server binary — the
    harness stand-in: the same JSON-RPC frames Claude Code / Codex send."""

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

    def _frames(self, name: str):
        self.ev.blob(name, "\n".join(json.dumps(f) for f in self.transcript))

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
            "clientInfo": {"name": "i0-scripted-harness", "version": "0"}})
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
        need(not is_error, f"{self.tag}: {name}: {text}")
        return json.loads(text)

    def close(self, frames_name: str | None = None):
        if frames_name:
            self._frames(frames_name)
        self.proc.stdin.close()
        self.proc.kill()
        self.proc.wait()


# --------------------------------------------------- byom scripted flow ----

def timeline(daemon: ByomDaemon, cursor: str) -> list:
    reply = daemon.expect_ok("projection", {
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


def sovereign_id(daemon: ByomDaemon, society_id: str) -> str:
    snap = daemon.expect_ok("projection", {
        "version": "0.2", "op": "snapshot_get", "society_id": society_id,
        "kinds": ["participants"]})
    for p in snap["result"]["participants"]:
        if p.get("kind") == "human":
            return p["participant_id"]
    raise Fail("no sovereign human participant in the snapshot")


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


BYOM_EXPECTED_ORDER = [
    "society.prepared", "society.genesis",
    "membership.offered", "membership.accepted", "membership.admitted",
    "manifestation.admitted",
    "mandate.prepared", "mandate.position_recorded", "mandate.issued",
    "activity.opened", "wake-intent.submitted",
    "endeavor.proposed", "endeavor.position_recorded", "endeavor.finalized",
    "call.opened",
    "pledge.proposed", "pledge.position_recorded", "pledge.committed",
    "pledge.underway", "delivery.submitted", "review.recorded",
]


def byom_scripted_flow(ev: Evidence, tag: str) -> dict:
    """The complete byom side of the tracer; returns the context the
    trail checks read. Transports are exactly the sheet's: MCP for
    candidate+agent, the direct human channel for governance+sovereign,
    the CLI for its verbs."""
    d = ByomDaemon(tag)
    env_cli = {"BYOM_RUNTIME_DIR": str(d.run_dir)}
    inc = d.incarnation()

    # 1-2. society_prepare -> society_bootstrap (governance surface,
    #      direct human channel, fresh challenge; atomic genesis).
    prepared = d.expect_ok("governance", {
        "version": "0.2", "op": "society_prepare",
        "meta": meta(inc, f"{tag}-prep"),
        "home_authority_ref": "auth-home-1",
        "proposed_charter_ref": "charter-draft-1",
        "proposed_charter_digest": digest(0xA1),
        "classification_binding_ref": "class-bind-1",
        "classification_binding_digest": digest(0xA2)})
    society = prepared["result"]["society_id"]
    booted = d.expect_ok("governance", {
        "version": "0.2", "op": "society_bootstrap",
        "meta": meta(inc, f"{tag}-boot", 1),
        "society_id": society,
        "preparation_ref": prepared["result"]["preparation_ref"],
        "subject_digest": prepared["result"]["subject_digest"]})
    genesis = booted["source_cursor"]
    ev.step("byom: society_prepare + society_bootstrap (governance, "
            "direct human channel) — atomic genesis",
            society_id=society, genesis_cursor=genesis)
    shown = cli([byom_cli_bin(), "society", "show", "--society", society],
                env_cli, ev, "byom-cli-society-show.txt")
    need('"state": "active"' in shown.stdout,
         f"CLI society show not active: {shown.stdout}")
    ev.step("byom: `byom society show` (CLI) confirms the active Society",
            state="active")

    # 3. membership_offer naming the proposed attached_harness
    #    ManifestationRevision (governance).
    subject = digest(0xB1)
    offered = d.expect_ok("governance", {
        "version": "0.2", "op": "membership_offer",
        "meta": meta(inc, f"{tag}-offer"),
        "participant_ref": AGENT,
        "proposed_standing_ref": "standing-proposal-1",
        "subject_digest": subject,
        "offered_by_decision_ref": f"dec-society-{society}",
        "expires_at": FAR_FUTURE})
    offer_id = offered["result"]["offer_id"]
    manifestation = None
    for e in timeline(d, genesis):
        if e["kind"] == "manifestation.proposed":
            manifestation = e["object_ref"]
            payload = d.expect_ok("projection", {
                "version": "0.2", "op": "event_payload",
                "event_id": e["event_id"]})
            need(payload["result"]["payload"].get("kind")
                 == "attached_harness",
                 f"manifestation not attached_harness: {payload}")
    need(manifestation is not None, "no manifestation.proposed event")
    token_file = d.token_file(f"candidate-{offer_id}.token")
    need(token_file.exists(), f"candidate token file missing: {token_file}")
    ev.step("byom: membership_offer (governance) minted the offer, the "
            "proposed attached_harness ManifestationRevision, and the "
            "candidate channel token",
            offer_id=offer_id, manifestation_ref=manifestation,
            token_file=str(token_file))

    # 3b. membership_accept over the REAL byom-mcp candidate profile —
    #     the scripted harness stand-in.
    cand = Mcp([byom_mcp_bin(), "--profile", "candidate"],
               {"BYOM_RUNTIME_DIR": str(d.run_dir),
                "BYOM_CANDIDATE_TOKEN_FILE": str(token_file)},
               ev, "byom-mcp[candidate]")
    names = [t["name"] for t in cand.tools()]
    need(names == ["byom_membership_refuse", "byom_membership_accept",
                   "byom_candidate_self_policy_propose"],
         f"candidate profile tools drifted: {names}")
    accepted = cand.call_ok("byom_membership_accept",
                            {"offer_ref": offer_id,
                             "subject_digest": subject})
    need(accepted["result"]["offer_state"] == "accepted",
         f"accept: {accepted}")
    need(accepted.get("revision") == 2, f"accept revision: {accepted}")
    acceptance = accepted["result"]["acceptance_id"]
    ev.step("byom: membership_accept via byom-mcp CANDIDATE profile "
            "(scripted MCP JSON-RPC over stdio; envelope, token preamble, "
            "expected_revision all bridge-derived)",
            tools=names, acceptance_id=acceptance,
            offer_state="accepted", revision=2)

    # 3c. participant_admit AND manifestation_admit (two governance
    #     decisions) -> active Standing; candidate channel closes.
    d.expect_ok("governance", {
        "version": "0.2", "op": "participant_admit",
        "meta": meta(inc, f"{tag}-admit", 2),
        "offer_ref": offer_id,
        "membership_acceptance_ref": acceptance,
        "admitted_by_decision_ref": f"dec-offer-{offer_id}",
        "admission_subject_digest": subject})
    d.expect_ok("governance", {
        "version": "0.2", "op": "manifestation_admit",
        "meta": meta(inc, f"{tag}-manif", 1),
        "manifestation_ref": manifestation,
        "admitted_by_decision_ref": f"dec-manif-{manifestation}"})
    text, is_error = cand.call("byom_membership_refuse", {
        "offer_ref": offer_id, "offer_subject_digest": subject,
        "superseded_acceptance_ref": acceptance})
    need(is_error and "https://byom.dev/problems/forbidden" in text,
         f"candidate channel must be closed after admission: {text}")
    cand.close("byom-mcp-candidate-frames.jsonl")
    agent_token = d.read_token(f"participant-{AGENT}.token")
    sov = sovereign_id(d, society)
    ev.step("byom: participant_admit + manifestation_admit (two "
            "governance decisions) — Standing active; candidate channel "
            "CLOSED (post-admission candidate call answers forbidden); "
            "participant channel minted",
            participant=AGENT, sovereign=sov,
            candidate_channel="closed (forbidden)")

    # 4. mandate chain: prepare [agent over MCP] -> position [human
    #    seat, fresh challenge] -> issue [governance].
    agent = Mcp([byom_mcp_bin(), "--profile", "participant"],
                {"BYOM_RUNTIME_DIR": str(d.run_dir),
                 "BYOM_PARTICIPANT_TOKEN": agent_token,
                 "BYOM_SOCIETY": society},
                ev, "byom-mcp[participant]")
    need(len(agent.tools()) == 34,
         "participant profile must expose exactly the 34 tools")
    mprep = agent.call_ok("byom_mandate_prepare", {
        "grantee_participant_ref": AGENT,
        "purpose_ref": "purpose-explore-1",
        "allowed_operations": ["activity_open", "continuation_write",
                               "wake_intent_submit"],
        "resource_selectors": ["res-repo-1"],
        "data_class_selectors": ["class-public"],
        "destination_selectors": [],
        "budget_ceiling_set_ref": "budget-mandate-1",
        "concurrency_ceiling": 2,
        "delegation": {"allowed": False, "max_depth": 0, "max_children": 0,
                       "grantee_selectors": []},
        "expires_at": FAR_FUTURE})
    mandate = mprep["result"]["mandate_id"]
    seat = mprep["result"]["required_seat_refs"][0]
    d.expect_ok("governance", {
        "version": "0.2", "op": "mandate_position",
        "meta": meta(inc, f"{tag}-mpos"),
        "proposal_ref": mandate, "proposal_revision": 1,
        "subject_digest": mprep["result"]["subject_digest"],
        "seat_ref": seat, "value": "assent"})
    d.expect_ok("governance", {
        "version": "0.2", "op": "mandate_issue",
        "meta": meta(inc, f"{tag}-missue", 1),
        "mandate_id": mandate,
        "subject_digest": mprep["result"]["subject_digest"]})
    ev.step("byom: mandate chain — mandate_prepare via byom-mcp "
            "PARTICIPANT profile; mandate_position (human seat, fresh "
            "challenge) + mandate_issue with budget reservation "
            "(governance)",
            mandate_id=mandate, seat_ref=seat)

    # 5. activity_open kind=exploration under the mandate [MCP];
    #    wake_intent_submit accepted and left PENDING.
    opened = agent.call_ok("byom_activity_open", {
        "kind": "exploration", "purpose_ref": "purpose-explore-1",
        "purpose_digest": digest(0xC0), "mandate_refs": [mandate],
        "budget_account_set_ref": "budget-mandate-1"})
    exploration = opened["result"]["activity_stream_id"]
    need(opened["result"]["state"] == "ready", f"exploration: {opened}")
    wake = agent.call_ok("byom_wake_intent_submit", {
        "activity_stream_ref": exploration, "generation": 1,
        "origin": "direct_participant",
        "exact_cause_ref": "cause-followup-1",
        "exact_cause_digest": digest(0xC2),
        "purpose_ref": "purpose-explore-1",
        "stable_wake_key": f"wake-{tag}", "expires_at": FAR_FUTURE})
    need(wake["result"]["state"] == "submitted", f"wake: {wake}")
    ev.step("byom: activity_open kind=exploration --mandate (via "
            "byom-mcp participant profile); wake_intent_submit accepted "
            "and left pending",
            activity_stream=exploration, wake_state="submitted")

    # 6a. endeavor propose/position/finalize + call_open — the human
    #     sovereign on the direct human channel (participant socket, no
    #     channel credential; byom-cli has no endeavor/pledge verbs yet).
    eprop = d.expect_ok("participant", {
        "version": "0.2", "op": "endeavor_propose",
        "meta": meta(inc, f"{tag}-eprop"),
        "purpose_ref": "purpose-improve-1",
        "purpose_digest": digest(0xD0),
        "sponsor_participant_refs": [sov],
        "governance_rule_set_ref": "rules-endeavor-1",
        "outcome_schema_refs": ["schema-change-set-1"],
        "acceptance_rule_ref": "rule-accept-1",
        "classification_join_ref": "class-join-1",
        "budget_account_set_ref": f"budget-endeavor-{tag}"})
    endeavor = eprop["result"]["endeavor_id"]
    sponsor_seat = eprop["result"]["required_seat_refs"][0]
    d.expect_ok("participant", {
        "version": "0.2", "op": "endeavor_position",
        "meta": meta(inc, f"{tag}-epos"),
        "proposal_ref": endeavor, "proposal_revision": 1,
        "subject_digest": eprop["result"]["subject_digest"],
        "seat_ref": sponsor_seat, "value": "assent"})
    d.expect_ok("participant", {
        "version": "0.2", "op": "endeavor_finalize",
        "meta": meta(inc, f"{tag}-efin", 1),
        "endeavor_id": endeavor,
        "subject_digest": eprop["result"]["subject_digest"]})
    call_opened = d.expect_ok("participant", {
        "version": "0.2", "op": "call_open",
        "meta": meta(inc, f"{tag}-call"),
        "endeavor_id": endeavor,
        "requested_outcome_schema_refs": ["schema-change-set-1"],
        "acceptance_criteria_refs": ["criteria-review-1"],
        "evidence_requirements": []})
    call_id = call_opened["result"]["call_id"]
    ev.step("byom: endeavor_propose/position/finalize + call_open "
            "(human sovereign, direct human channel)",
            endeavor_id=endeavor, call_id=call_id)

    # 6b. pledge_propose [agent over MCP].
    pprop = agent.call_ok("byom_pledge_propose", {
        "endeavor_id": endeavor, "call_ref": call_id,
        "proposed_pledgor_ref": AGENT, "beneficiary_ref": sov,
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
    ev.step("byom: pledge_propose via byom-mcp participant profile "
            "(agent) — required seats minted",
            proposal_id=proposal, required_slots=sorted(slots))

    # 6c. pledge_position for every required seat: the pledgor over MCP,
    #     the beneficiary HUMAN seat on the direct human channel.
    agent.call_ok("byom_pledge_position", {
        "proposal_ref": proposal, "proposal_revision": 1,
        "subject_digest": terms, "seat_ref": slots["pledgor_assent"],
        "value": "assent", "assent_mode": "direct_participant"})
    d.expect_ok("participant", {
        "version": "0.2", "op": "pledge_position",
        "meta": meta(inc, f"{tag}-ppos-sov"),
        "proposal_ref": proposal, "proposal_revision": 1,
        "subject_digest": terms, "seat_ref": slots["beneficiary_assent"],
        "value": "assent", "assent_mode": "direct_participant"})
    finalized = agent.call_ok("byom_pledge_finalize", {
        "proposal_ref": proposal, "proposal_revision": 1,
        "subject_digest": terms})
    pledge = finalized["result"]["pledge_id"]
    ev.step("byom: pledge_position pledgor seat (agent via MCP) + "
            "beneficiary seat (human, direct human channel); "
            "pledge_finalize (deterministic, via MCP)",
            pledge_id=pledge)

    # 6d. deterministic delivery under a pledge_work stream [MCP], then
    #     review_record [human].
    work = agent.call_ok("byom_activity_open", {
        "kind": "pledge_work", "purpose_ref": "purpose-improve-1",
        "purpose_digest": digest(0xD4),
        "pledge_binding": {"pledge_id": pledge, "pledge_revision": 1,
                           "terms_digest": terms},
        "mandate_refs": [],
        "budget_account_set_ref": f"budget-endeavor-{tag}"})
    work_stream = work["result"]["activity_stream_id"]
    delivered = agent.call_ok("byom_delivery_submit", {
        "pledge_id": pledge, "pledge_revision": 2, "terms_digest": terms,
        "output_refs": ["change-set-1"],
        "evidence_refs": ["attest-complete-readable-source-1"],
        "activity_stream_ref": work_stream})
    delivery = delivered["result"]["delivery_id"]
    reviewed = d.expect_ok("participant", {
        "version": "0.2", "op": "review_record",
        "meta": meta(inc, f"{tag}-review"),
        "pledge_id": pledge,
        "pledge_revision": delivered["result"]["pledge_revision"],
        "delivery_id": delivery,
        "reviewed_subject_digest": delivered["result"]["subject_digest"],
        "outcome": "fulfilled",
        "decision_or_mandate_use_ref": "dec-review-1"})
    need(reviewed["result"]["pledge_state"] == "fulfilled",
         f"review: {reviewed}")
    agent.close("byom-mcp-participant-frames.jsonl")
    ev.step("byom: delivery_submit (agent via MCP) -> review_record "
            "(human, direct human channel) — pledge fulfilled",
            delivery_id=delivery,
            review_id=reviewed["result"]["review_id"],
            pledge_state="fulfilled")

    # 7 (byom half). The per-source trail: events_read from genesis.
    events = timeline(d, genesis)
    kinds = [e["kind"] for e in events]
    assert_ordered(kinds, BYOM_EXPECTED_ORDER, "byom")
    forbidden = [k for k in kinds
                 if k.startswith(("episode.", "placement.", "activation."))
                 or k == "wake-intent.activated"]
    need(not forbidden,
         f"I0 excludes activation/placement/episodes, saw {forbidden}")
    need(kinds.count("wake-intent.submitted") == 1
         and not any(k.startswith("wake-intent.")
                     and k != "wake-intent.submitted" for k in kinds),
         "wake intent must remain pending (submitted, nothing after)")
    events_cli = cli([byom_cli_bin(), "events", "--cursor", genesis],
                     env_cli, ev, "byom-cli-events.txt")
    need('"events"' in events_cli.stdout, "CLI events read failed")
    ev.step("byom: per-source trail — events_read timeline kinds in "
            "order from genesis (socket assertion + `byom events` CLI "
            "lens); wake intent still pending; no activation/placement/"
            "episode events",
            timeline_kinds=kinds)

    return {"daemon": d, "society": society, "genesis": genesis,
            "sovereign": sov, "offer": offer_id, "mandate": mandate,
            "endeavor": endeavor, "pledge": pledge, "delivery": delivery,
            "events": events}


# -------------------------------------------------- kovee scripted flow ----

KOVEE_EXPECTED_TYPES = [
    "dev.kovee.project.created.v1",
    "dev.kovee.space.created.v1",
    "dev.kovee.space.contribution-appended.v1",   # the question (CLI)
    "dev.kovee.space.contribution-appended.v1",   # MCP round-trip 1
    "dev.kovee.space.contribution-appended.v1",   # MCP round-trip 2
]


def kovee_scripted_flow(ev: Evidence, tag: str) -> dict:
    k = Koveed(tag)
    env_cli = {"KOVEE_RUNTIME_DIR": str(k.run_dir)}

    # kovee init (CLI): daemon reachable, personal realm, default project.
    init = cli([kovee_cli_bin(), "init"], env_cli, ev, "kovee-cli-init.txt")
    match = re.search(r"project:\s+(\S+)", init.stdout)
    need(match, f"kovee init printed no project: {init.stdout}")
    project = match.group(1)
    ev.step("kovee: `kovee init` (CLI) — personal realm + default project",
            project_id=project)

    # space create (CLI).
    created = cli([kovee_cli_bin(), "space", "create", "--project", project,
                   "--title", "I0 society-of-two"],
                  env_cli, ev, "kovee-cli-space-create.txt")
    space_result = json.loads(created.stdout)
    space = space_result["space_id"]
    branch = space_result["main_branch_id"]
    ev.step("kovee: `kovee space create` (CLI)", space_id=space,
            main_branch_id=branch)

    # question contribution (CLI; the CLI derives the branch head from
    # the event ledger itself).
    question = cli([kovee_cli_bin(), "space", "contribute",
                    "--project", project, "--space", space,
                    "--kind", "question",
                    "--text", "What belongs in the I0 tracer?"],
                   env_cli, ev, "kovee-cli-question.txt")
    q_result = json.loads(question.stdout)
    need(q_result["kind"] == "question", f"question kind: {q_result}")
    ev.step("kovee: question contribution (CLI, kind=question)",
            contribution_id=q_result["contribution_id"],
            branch_sequence=q_result["origin_branch_sequence"])

    # The contribution round-trip via the REAL kovee-mcp, scripted the
    # same way as the byom harness stand-in: derive the branch head as
    # an authorized reader (events fold), append, verify the head chain.
    mcp = Mcp([kovee_mcp_bin()],
              {"KOVEE_RUNTIME_DIR": str(k.run_dir),
               "KOVEE_PROJECT": project},
              ev, "kovee-mcp")
    need(len(mcp.tools()) == 14,
         "kovee-mcp must expose exactly the 14 participant tools")
    shown = mcp.call_ok("kovee_space_show", {"space_id": space})
    need(shown["main_branch_id"] == branch, f"space_show: {shown}")
    events = mcp.call_ok("kovee_events_read", {
        "source": project, "limit": 512,
        "type_prefixes": ["dev.kovee.space.contribution-appended.v1"]})
    head = kovee_genesis_head(branch)
    entries = []
    for e in events["events"]:
        p = e.get("payload") or {}
        if p.get("origin_branch_id") == branch:
            entries.append((p["origin_branch_sequence"],
                            p["content_digest"]))
    for seq, cdigest in sorted(entries):
        head = kovee_next_head(head, seq, cdigest)
    appended = mcp.call_ok("kovee_contribution_append", {
        "space_id": space, "branch_id": branch,
        "expected_head_digest": head, "kind": "utterance",
        "body_parts": [{"media_type": "text/plain",
                        "text": "Scripted kovee-mcp round-trip: the two "
                                "stacks stay separate in I0."}]})
    # The deterministic acceptance of the §10.3 chain: predict the next
    # head locally and append AGAINST it — koveed CASes the head, so a
    # divergent fold would answer stale_revision, and success proves the
    # daemon and this client derive the identical chain.
    predicted = kovee_next_head(head, appended["origin_branch_sequence"],
                                appended["content_digest"])
    answered = mcp.call_ok("kovee_contribution_append", {
        "space_id": space, "branch_id": branch,
        "expected_head_digest": predicted, "kind": "synthesis",
        "body_parts": [{"media_type": "text/plain",
                        "text": "The I0 tracer holds one byom Society "
                                "and one kovee space, trails separate."}]})
    listing = mcp.call_ok("kovee_contribution_list",
                          {"space_id": space, "limit": 100})
    listed_kinds = [c["kind"] for c in listing["items"]]
    need(sorted(listed_kinds) == ["question", "synthesis", "utterance"],
         f"contribution kinds: {listed_kinds}")
    mcp.close("kovee-mcp-frames.jsonl")
    ev.step("kovee: contribution round-trip via the REAL kovee-mcp "
            "(scripted MCP JSON-RPC; branch head derived from the event "
            "fold, second append CASed against the locally predicted "
            "next head — the deterministic §10.3 chain agreement)",
            mcp_contribution_ids=[appended["contribution_id"],
                                  answered["contribution_id"]],
            predicted_head=predicted,
            contribution_kinds=sorted(listed_kinds))

    # kovee per-source trail: its OWN events, separately (no merged view).
    reply = k.expect_ok(kv_read("events_read", project,
                                {"source": project, "limit": 512}))
    kev = reply["result"]["events"]
    types = [e["type"] for e in kev]
    assert_ordered(types, KOVEE_EXPECTED_TYPES, "kovee")
    for i, e in enumerate(kev):
        need(e.get("project_sequence") == i + 1,
             f"kovee project sequences not dense at {i}: {e}")
        need(e.get("actor_ref"), f"kovee event without actor_ref: {e}")
    events_cli = cli([kovee_cli_bin(), "events", "--project", project],
                     env_cli, ev, "kovee-cli-events.txt")
    need("dev.kovee.space.contribution-appended.v1" in events_cli.stdout,
         "CLI events lens missing the contributions")
    ev.step("kovee: per-source trail — kovee events over kovee records "
            "only (dense project sequences, every event attributed); "
            "asserted separately from the byom timeline, no merged view",
            event_types=types)

    return {"daemon": k, "project": project, "space": space,
            "branch": branch, "events": kev}


# ------------------------------------------------------------ scripted ----

def mode_scripted() -> int:
    ev = Evidence("i0-flow-scripted")
    print("i0-flow-scripted: society-of-two, both daemons live, "
          "byom-mcp/kovee-mcp as the scripted harness")
    byom_ctx = kovee_ctx = None
    try:
        byom_ctx = byom_scripted_flow(ev, "i0s")
        kovee_ctx = kovee_scripted_flow(ev, "i0s")
        ev.step("society-of-two: both stacks completed their flows in "
                "one scenario run; per-source trails asserted "
                "independently",
                byom_society=byom_ctx["society"],
                kovee_project=kovee_ctx["project"])
        print(f"i0-flow-scripted: PASS ({ev.n} steps; evidence "
              f"{ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()
        if byom_ctx:
            byom_ctx["daemon"].cleanup()
        if kovee_ctx:
            kovee_ctx["daemon"].cleanup()


# -------------------------------------------------------- crash matrix ----

# The record each armed operation must produce EXACTLY once — the
# absence probe of a pre-witness crash and the presence probe after the
# retry read this map, so no cell can pass by checking nothing.
CRASH_EFFECT_KIND = {
    "membership_offer": "membership.offered",
    "membership_accept": "membership.accepted",
    "participant_admit": "membership.admitted",
    "pledge_finalize": "pledge.committed",
    "delivery_submit": "delivery.submitted",
}


def count_kind(d: ByomDaemon, cursor: str, kind: str) -> int:
    return sum(1 for e in timeline(d, cursor) if e["kind"] == kind)


# The byom direct-socket flow used by the crash matrix: pinned
# idempotency keys so the exact retry after a kill is the SAME command
# (the b1_crash_matrix discipline). Candidate/participant calls present
# a FRESH sender-constrained proof per transmission (the request line
# itself is byte-identical across retry and replay — the proof rides the
# preamble, so idempotency and byte-identical replies still hold).
def byom_crash_flow(cell: str, d: ByomDaemon, crash_op: str,
                    crash_phase: str, ev: Evidence) -> dict:
    inc = d.incarnation()
    state = {"crashed": False, "genesis": None}

    def send(op, surface, request, cred=None):
        line = json.dumps(request)

        def proof():
            return None if cred is None else d.proof(cred, op)

        if op == crash_op and not state["crashed"]:
            kind = CRASH_EFFECT_KIND[op]
            probe = crash_phase == "before_witness" and state["genesis"]
            baseline = count_kind(d, state["genesis"], kind) if probe else None
            raw = d.call_raw(surface, line, proof())
            need(raw is None,
                 f"{cell}: {op} must die at the armed point, got {raw}")
            d.wait_exit()
            d.restart()
            state["crashed"] = True
            if probe:
                # The daemon died AFTER the SQL prepare and BEFORE the
                # witness CAS: the journal never saw the transaction, so
                # startup must abandon the inert pending state. Asserted
                # BEFORE any retry — otherwise a replayed result would
                # mask a record that had leaked out early.
                leaked = count_kind(d, state["genesis"], kind)
                need(leaked == baseline,
                     f"{cell}: a pre-witness crash must leave no {kind} "
                     f"record: {leaked} (baseline {baseline})")
                minted = sorted(p.name for p in
                                (d.data_dir / "channels").glob("*"))
                need(not minted,
                     f"{cell}: a pre-witness crash must mint no channel "
                     f"credential, found {minted}")
                ev.step(f"{cell}: killed byomd BEFORE the witness CAS; "
                        f"after restart the record is ABSENT — no {kind} "
                        "event and no credential file — checked BEFORE "
                        "any retry",
                        absent_kind=kind, count_before_retry=leaked,
                        credentials_on_disk=minted)
            first = d.call_raw(surface, line, proof())
            need(first is not None, f"{cell}: {op} retry got no reply")
            reply = json.loads(first)
            need(reply.get("outcome") == "ok",
                 f"{cell}: {op} retry: {first}")
            if probe:
                after = count_kind(d, state["genesis"], kind)
                need(after == baseline + 1,
                     f"{cell}: the retry must produce exactly one {kind}: "
                     f"{after}")
            second = d.call_raw(surface, line, proof())
            need(first == second,
                 f"{cell}: {op} replay must be byte-identical")
            if probe:
                need(count_kind(d, state["genesis"], kind) == baseline + 1,
                     f"{cell}: a replay must commit nothing new")
            ev.step(f"{cell}: killed byomd mid-{op}, restarted; exact "
                    "retry ok; second replay byte-identical",
                    op=op)
            return reply
        raw = d.call_raw(surface, line, proof())
        need(raw is not None, f"{cell}: {op} died unexpectedly")
        reply = json.loads(raw)
        need(reply.get("outcome") == "ok", f"{cell}: {op}: {raw}")
        return reply

    tag = cell
    prepared = send("society_prepare", "governance", {
        "version": "0.2", "op": "society_prepare",
        "meta": meta(inc, f"{tag}-prep"),
        "home_authority_ref": "auth-home-1",
        "proposed_charter_ref": "charter-draft-1",
        "proposed_charter_digest": digest(0xA1),
        "classification_binding_ref": "class-bind-1",
        "classification_binding_digest": digest(0xA2)})
    society = prepared["result"]["society_id"]
    booted = send("society_bootstrap", "governance", {
        "version": "0.2", "op": "society_bootstrap",
        "meta": meta(inc, f"{tag}-boot", 1),
        "society_id": society,
        "preparation_ref": prepared["result"]["preparation_ref"],
        "subject_digest": prepared["result"]["subject_digest"]})
    genesis = booted["source_cursor"]
    state["genesis"] = genesis
    subject = digest(0xB1)
    offered = send("membership_offer", "governance", {
        "version": "0.2", "op": "membership_offer",
        "meta": meta(inc, f"{tag}-offer"),
        "participant_ref": AGENT,
        "proposed_standing_ref": "standing-proposal-1",
        "subject_digest": subject,
        "offered_by_decision_ref": f"dec-society-{society}",
        "expires_at": FAR_FUTURE})
    offer_id = offered["result"]["offer_id"]
    cand_cred = f"candidate-{offer_id}.token"
    accepted = send("membership_accept", "candidate", {
        "version": "0.2", "op": "membership_accept",
        "meta": meta(inc, f"{tag}-accept", 1),
        "offer_ref": offer_id, "subject_digest": subject}, cand_cred)
    send("participant_admit", "governance", {
        "version": "0.2", "op": "participant_admit",
        "meta": meta(inc, f"{tag}-admit", 2),
        "offer_ref": offer_id,
        "membership_acceptance_ref": accepted["result"]["acceptance_id"],
        "admitted_by_decision_ref": f"dec-offer-{offer_id}",
        "admission_subject_digest": subject})
    manifestation = None
    for e in timeline(d, genesis):
        if e["kind"] == "manifestation.proposed":
            manifestation = e["object_ref"]
    send("manifestation_admit", "governance", {
        "version": "0.2", "op": "manifestation_admit",
        "meta": meta(inc, f"{tag}-manif", 1),
        "manifestation_ref": manifestation,
        "admitted_by_decision_ref": f"dec-manif-{manifestation}"})
    agent_cred = f"participant-{AGENT}.token"
    sov = sovereign_id(d, society)
    mprep = send("mandate_prepare", "participant", {
        "version": "0.2", "op": "mandate_prepare",
        "meta": meta(inc, f"{tag}-mprep"),
        "grantee_participant_ref": AGENT,
        "purpose_ref": "purpose-explore-1",
        "allowed_operations": ["activity_open", "continuation_write",
                               "wake_intent_submit"],
        "resource_selectors": ["res-repo-1"],
        "data_class_selectors": ["class-public"],
        "destination_selectors": [],
        "budget_ceiling_set_ref": "budget-mandate-1",
        "concurrency_ceiling": 2,
        "delegation": {"allowed": False, "max_depth": 0,
                       "max_children": 0, "grantee_selectors": []},
        "expires_at": FAR_FUTURE}, agent_cred)
    mandate = mprep["result"]["mandate_id"]
    send("mandate_position", "governance", {
        "version": "0.2", "op": "mandate_position",
        "meta": meta(inc, f"{tag}-mpos"),
        "proposal_ref": mandate, "proposal_revision": 1,
        "subject_digest": mprep["result"]["subject_digest"],
        "seat_ref": mprep["result"]["required_seat_refs"][0],
        "value": "assent"})
    send("mandate_issue", "governance", {
        "version": "0.2", "op": "mandate_issue",
        "meta": meta(inc, f"{tag}-missue", 1),
        "mandate_id": mandate,
        "subject_digest": mprep["result"]["subject_digest"]})
    opened = send("activity_open", "participant", {
        "version": "0.2", "op": "activity_open",
        "meta": meta(inc, f"{tag}-explore"),
        "kind": "exploration", "purpose_ref": "purpose-explore-1",
        "purpose_digest": digest(0xC0), "mandate_refs": [mandate],
        "budget_account_set_ref": "budget-mandate-1"}, agent_cred)
    send("wake_intent_submit", "participant", {
        "version": "0.2", "op": "wake_intent_submit",
        "meta": meta(inc, f"{tag}-wake"),
        "activity_stream_ref": opened["result"]["activity_stream_id"],
        "generation": 1, "origin": "direct_participant",
        "exact_cause_ref": "cause-followup-1",
        "exact_cause_digest": digest(0xC2),
        "purpose_ref": "purpose-explore-1",
        "stable_wake_key": f"wake-{tag}",
        "expires_at": FAR_FUTURE}, agent_cred)
    eprop = send("endeavor_propose", "participant", {
        "version": "0.2", "op": "endeavor_propose",
        "meta": meta(inc, f"{tag}-eprop"),
        "purpose_ref": "purpose-improve-1",
        "purpose_digest": digest(0xD0),
        "sponsor_participant_refs": [sov],
        "governance_rule_set_ref": "rules-endeavor-1",
        "outcome_schema_refs": ["schema-change-set-1"],
        "acceptance_rule_ref": "rule-accept-1",
        "classification_join_ref": "class-join-1",
        "budget_account_set_ref": f"budget-endeavor-{tag}"})
    endeavor = eprop["result"]["endeavor_id"]
    send("endeavor_position", "participant", {
        "version": "0.2", "op": "endeavor_position",
        "meta": meta(inc, f"{tag}-epos"),
        "proposal_ref": endeavor, "proposal_revision": 1,
        "subject_digest": eprop["result"]["subject_digest"],
        "seat_ref": eprop["result"]["required_seat_refs"][0],
        "value": "assent"})
    send("endeavor_finalize", "participant", {
        "version": "0.2", "op": "endeavor_finalize",
        "meta": meta(inc, f"{tag}-efin", 1),
        "endeavor_id": endeavor,
        "subject_digest": eprop["result"]["subject_digest"]})
    call_opened = send("call_open", "participant", {
        "version": "0.2", "op": "call_open",
        "meta": meta(inc, f"{tag}-call"),
        "endeavor_id": endeavor,
        "requested_outcome_schema_refs": ["schema-change-set-1"],
        "acceptance_criteria_refs": ["criteria-review-1"],
        "evidence_requirements": []})
    pprop = send("pledge_propose", "participant", {
        "version": "0.2", "op": "pledge_propose",
        "meta": meta(inc, f"{tag}-pprop"),
        "endeavor_id": endeavor,
        "call_ref": call_opened["result"]["call_id"],
        "proposed_pledgor_ref": AGENT, "beneficiary_ref": sov,
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
        "dependency_refs": []}, agent_cred)
    proposal = pprop["result"]["proposal_id"]
    terms = pprop["result"]["terms_digest"]
    slots = {s["kind"]: s["seat_refs"][0]
             for s in pprop["result"]["required_slots"]}
    send("pledge_position", "participant", {
        "version": "0.2", "op": "pledge_position",
        "meta": meta(inc, f"{tag}-ppos-agent"),
        "proposal_ref": proposal, "proposal_revision": 1,
        "subject_digest": terms, "seat_ref": slots["pledgor_assent"],
        "value": "assent", "assent_mode": "direct_participant"},
        agent_cred)
    send("pledge_position", "participant", {
        "version": "0.2", "op": "pledge_position",
        "meta": meta(inc, f"{tag}-ppos-sov"),
        "proposal_ref": proposal, "proposal_revision": 1,
        "subject_digest": terms, "seat_ref": slots["beneficiary_assent"],
        "value": "assent", "assent_mode": "direct_participant"})
    finalized = send("pledge_finalize", "participant", {
        "version": "0.2", "op": "pledge_finalize",
        "meta": meta(inc, f"{tag}-pfin", 1),
        "proposal_ref": proposal, "proposal_revision": 1,
        "subject_digest": terms}, agent_cred)
    pledge = finalized["result"]["pledge_id"]
    work = send("activity_open", "participant", {
        "version": "0.2", "op": "activity_open",
        "meta": meta(inc, f"{tag}-work"),
        "kind": "pledge_work", "purpose_ref": "purpose-improve-1",
        "purpose_digest": digest(0xD4),
        "pledge_binding": {"pledge_id": pledge, "pledge_revision": 1,
                           "terms_digest": terms},
        "mandate_refs": [],
        "budget_account_set_ref": f"budget-endeavor-{tag}"}, agent_cred)
    delivered = send("delivery_submit", "participant", {
        "version": "0.2", "op": "delivery_submit",
        "meta": meta(inc, f"{tag}-deliver"),
        "pledge_id": pledge, "pledge_revision": 2,
        "terms_digest": terms, "output_refs": ["change-set-1"],
        "evidence_refs": ["attest-complete-readable-source-1"],
        "activity_stream_ref": work["result"]["activity_stream_id"]},
        agent_cred)
    reviewed = send("review_record", "participant", {
        "version": "0.2", "op": "review_record",
        "meta": meta(inc, f"{tag}-review"),
        "pledge_id": pledge,
        "pledge_revision": delivered["result"]["pledge_revision"],
        "delivery_id": delivered["result"]["delivery_id"],
        "reviewed_subject_digest": delivered["result"]["subject_digest"],
        "outcome": "fulfilled",
        "decision_or_mandate_use_ref": "dec-review-1"})
    need(reviewed["result"]["pledge_state"] == "fulfilled",
         f"{cell}: review")
    need(state["crashed"], f"{cell}: the flow never reached {crash_op}")
    return {"society": society, "genesis": genesis, "pledge": pledge}


# Exactly-once probes: the record classes the sheet forbids duplicating.
UNIQUE_BYOM_KINDS = [
    "membership.offered",      # offer
    "membership.accepted",
    "membership.admitted",     # admission
    "manifestation.admitted",
    "mandate.issued",          # mandate
    "pledge.committed",        # pledge
    "delivery.submitted",      # delivery
    "review.recorded",         # review
]

# Every §15.3 byom journal commit point is armed at least once:
# before_witness (nothing journaled), after_witness (entry without
# finalize), before_finalize (inside the finalize transaction),
# after_finalize (committed, reply lost).
BYOM_CRASH_CELLS = [
    ("membership_offer", "before_witness"),
    ("membership_accept", "after_witness"),
    ("participant_admit", "after_finalize"),
    ("pledge_finalize", "before_finalize"),
    ("delivery_submit", "after_finalize"),
]

KOVEE_CRASH_CELLS = [
    ("space_create", "before_commit"),
    ("contribution_append", "before_commit"),
    ("contribution_append", "after_commit"),
]


def kovee_crash_cell(op: str, phase: str, ev: Evidence):
    cell = f"kovee/{op}@{phase}"
    k = Koveed(f"cm-{op}-{phase}")
    try:
        project = k.expect_ok(kv_mutation(
            "project_create", None, f"idem-{op}-{phase}-project",
            {"name": "personal"}))["result"]["project_id"]
        if op == "space_create":
            target = kv_mutation("space_create", project,
                                 f"idem-{op}-{phase}-space",
                                 {"title": "Crash", "visibility": "project"})
            event_type = "dev.kovee.space.created.v1"
        else:
            space = k.expect_ok(kv_mutation(
                "space_create", project, f"idem-{op}-{phase}-space",
                {"title": "Crash", "visibility": "project"}))["result"]
            head = kovee_genesis_head(space["main_branch_id"])
            target = kv_mutation(
                "contribution_append", project, f"idem-{op}-{phase}-contrib",
                {"space_id": space["space_id"],
                 "branch_id": space["main_branch_id"],
                 "expected_head_digest": head, "kind": "question",
                 "body_parts": [{"media_type": "text/plain",
                                 "text": "does the crash duplicate me?"}]})
            event_type = "dev.kovee.space.contribution-appended.v1"

        def count():
            reply = k.expect_ok(kv_read("events_read", project,
                                        {"source": project, "limit": 512}))
            return sum(1 for e in reply["result"]["events"]
                       if e["type"] == event_type)

        before = count()
        # Arm, fire, die.
        k.kill()
        k.start({"KOVEED_ABORT": f"{phase}:{op}"})
        raw = k.call_raw(json.dumps(target))
        need(raw is None, f"{cell}: armed daemon must die, got {raw}")
        k.wait_exit()
        k.restart()
        after_crash = count()
        if phase == "before_commit":
            need(after_crash == before,
                 f"{cell}: nothing may survive a pre-commit abort")
        else:
            need(after_crash == before + 1,
                 f"{cell}: the committed transaction survives exactly once")
        first = k.call_raw(json.dumps(target))
        need(first is not None
             and json.loads(first).get("outcome") == "ok",
             f"{cell}: retry failed: {first}")
        need(count() == before + 1,
             f"{cell}: exactly one committed effect after the retry")
        second = k.call_raw(json.dumps(target))
        need(first == second, f"{cell}: replays must be byte-identical")
        need(count() == before + 1, f"{cell}: a replay commits nothing new")
        reply = k.expect_ok(kv_read("events_read", project,
                                    {"source": project, "limit": 512}))
        for i, e in enumerate(reply["result"]["events"]):
            need(e.get("project_sequence") == i + 1,
                 f"{cell}: sequences not dense after the crash cycle")
        ev.step(f"{cell}: killed koveed at the armed point, restarted; "
                "no duplicate, same-key retry ok, byte-identical replay, "
                "dense sequences",
                committed_after_crash=after_crash - before,
                committed_after_retry=1)
    finally:
        k.cleanup()


def mode_crash_matrix() -> int:
    ev = Evidence("i0-crash")
    print("i0-crash: kill/restart both daemons at armed mid-flow commit "
          "points (BYOMD_ABORT / KOVEED_ABORT)")
    try:
        for op, phase in BYOM_CRASH_CELLS:
            cell = f"byom/{op}@{phase}"
            d = ByomDaemon(f"cm-{op}-{phase}",
                           {"BYOMD_ABORT": f"{phase}:{op}"})
            try:
                ctx = byom_crash_flow(cell, d, op, phase, ev)
                counts = {}
                for e in timeline(d, ctx["genesis"]):
                    counts[e["kind"]] = counts.get(e["kind"], 0) + 1
                dupes = {kind: counts.get(kind, 0)
                         for kind in UNIQUE_BYOM_KINDS
                         if counts.get(kind, 0) != 1}
                need(not dupes,
                     f"{cell}: duplicated records after crash: {dupes}")
                snap = d.expect_ok("projection", {
                    "version": "0.2", "op": "snapshot_get",
                    "society_id": ctx["society"], "kinds": ["pledges"]})
                pledges = snap["result"]["pledges"]
                need(len(pledges) == 1
                     and pledges[0]["state"] == "fulfilled",
                     f"{cell}: pledge state after crash: {pledges}")
                ev.step(f"{cell}: flow completed after the crash — no "
                        "duplicated offer/admission/mandate/pledge/"
                        "delivery/review; single pledge fulfilled",
                        unique_counts={k: counts.get(k, 0)
                                       for k in UNIQUE_BYOM_KINDS})
            finally:
                d.cleanup()
        for op, phase in KOVEE_CRASH_CELLS:
            kovee_crash_cell(op, phase, ev)
        print(f"i0-crash: PASS ({len(BYOM_CRASH_CELLS)} byomd cells + "
              f"{len(KOVEE_CRASH_CELLS)} koveed cells; evidence "
              f"{ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()


# ------------------------------------------------------- trail checking ----

# Which actor authors each byom event kind in this scenario — the
# attribution the sheet's i0-trails verifies. The map is EXHAUSTIVE over
# the flow: --verify-trails FAILS on any event kind it does not list, so
# a record can never pass unchecked (I0-2). Each value is
# (label, predicate(actor_ref, payload) -> bool); the label lands in the
# evidence table beside the observed actor.
def byom_attribution_rules(sov: str) -> dict:
    gov_actor = GOV_ACTOR
    agent_actor = f"participant:{AGENT}"
    human_actor = f"participant:{sov}"

    def governance(a, _p):
        return a == gov_actor

    def candidate(a, _p):
        return a.startswith("candidate:")

    def agent(a, _p):
        return a == agent_actor

    def human(a, _p):
        return a == human_actor

    def budget_holder(a, p):
        """A reservation is authored by whoever caused it: the mandate
        reservation rides mandate_issue (governance), the pledge
        reservation rides pledge_finalize (the agent)."""
        holder = p.get("holder", "")
        if holder.startswith("mnd-"):
            return governance(a, p)
        if holder.startswith("plg-"):
            return agent(a, p)
        return False

    def pledge_seat(a, p):
        """The two pledge seats are two different actors on two
        different channels."""
        seat = p.get("seat_ref", "")
        if seat.startswith("seat-pledgor-"):
            return agent(a, p)
        if seat.startswith("seat-beneficiary-"):
            return human(a, p)
        return False

    g = (gov_actor, governance)
    c = ("candidate:<channel>", candidate)
    a_ = (agent_actor, agent)
    h = (human_actor, human)
    return {
        # genesis, charter and the sovereign's own Standing
        "society.prepared": g,
        "society.genesis": g,
        "charter.adopted": g,
        "participant.admitted": g,
        "standing.activated": g,
        "budget.roots_established": g,
        # onboarding
        "membership.offered": g,
        "participant.proposed": g,
        "manifestation.proposed": g,
        "channel.candidate_minted": g,
        "membership.accepted": c,
        "membership.admitted": g,
        "channel.converted": g,
        "manifestation.admitted": g,
        # mandate chain
        "mandate.prepared": a_,
        "mandate.position_recorded": g,
        "mandate.issued": g,
        "budget.reserved": (f"{gov_actor} (mandate holder) / "
                            f"{agent_actor} (pledge holder)", budget_holder),
        # the agent's activity
        "activity.opened": a_,
        "wake-intent.submitted": a_,
        # the human sovereign's endeavor
        "endeavor.proposed": h,
        "endeavor.position_recorded": h,
        "endeavor.finalized": h,
        "budget.delegated": h,
        "call.opened": h,
        # the pledge
        "pledge.proposed": a_,
        "pledge.position_recorded": (f"{agent_actor} (pledgor seat) / "
                                     f"{human_actor} (beneficiary seat)",
                                     pledge_seat),
        "pledge.committed": a_,
        "pledge.underway": a_,
        # delivery, review, settlement
        "delivery.submitted": a_,
        "review.recorded": h,
        "budget.settled": h,
    }


def verify_byom_attribution(d: ByomDaemon, events: list, sov: str) -> list:
    """Every byom record, checked against the exhaustive actor map. An
    unmapped kind is a FAILURE, never a skip; payloads are fetched so
    the kinds authored by two different seats are pinned exactly."""
    rules = byom_attribution_rules(sov)
    table = []
    for e in events:
        actor = e.get("actor_ref") or ""
        need(actor, f"byom event without actor_ref: {e}")
        need(e.get("causation_ref") and e.get("correlation_ref"),
             f"byom event without causal attribution: {e}")
        rule = rules.get(e["kind"])
        need(rule is not None,
             f"byom event kind {e['kind']!r} is absent from the trail "
             "actor map — the map must be exhaustive, an unmapped kind "
             "is never skipped")
        label, predicate = rule
        payload = d.expect_ok("projection", {
            "version": "0.2", "op": "event_payload",
            "event_id": e["event_id"]})["result"]["payload"]
        need(predicate(actor, payload),
             f"byom {e['kind']} attributed to {actor!r} — expected "
             f"{label}")
        table.append({"kind": e["kind"], "actor_ref": actor,
                      "expected": label, "checked": True})
    unused = sorted(set(rules) - {e["kind"] for e in events})
    need(not unused,
         f"the trail actor map lists kinds this flow never produced: "
         f"{unused} — the map tracks the flow exactly")
    return table


def mode_verify_trails() -> int:
    ev = Evidence("i0-trails")
    print("i0-trails: per-source attribution — every record's authoring "
          "surface/actor, byom and kovee separately")
    byom_ctx = kovee_ctx = None
    try:
        byom_ctx = byom_scripted_flow(ev, "i0t")
        kovee_ctx = kovee_scripted_flow(ev, "i0t")

        # -- byom: every event names its authoring actor; the flow's
        #    kinds attribute to exactly the channel that authored them.
        sov = byom_ctx["sovereign"]
        table = verify_byom_attribution(byom_ctx["daemon"],
                                        byom_ctx["events"], sov)
        positions = [e["actor_ref"] for e in byom_ctx["events"]
                     if e["kind"] == "pledge.position_recorded"]
        need(sorted(positions) == sorted([f"participant:{AGENT}",
                                          f"participant:{sov}"]),
             f"pledge positions must come from both seats: {positions}")
        ev.blob("byom-attribution.json", json.dumps(table, indent=2))
        ev.step("byom: every event carries actor_ref + causation/"
                "correlation; EVERY kind is checked against the "
                "exhaustive actor map (governance:sovereign / "
                f"candidate:<chan> / participant:{AGENT} / "
                "participant:<sovereign>) — an unmapped kind fails; the "
                "two pledge seats and the two budget reservations are "
                "distinct actors",
                events=len(byom_ctx["events"]),
                kinds_checked=len(table),
                distinct_kinds=len({r["kind"] for r in table}),
                unchecked=0,
                pledge_position_actors=sorted(positions))

        # The map's own negative: a kind the map does not list must FAIL
        # the check — the silent skip I0-2 found is gone.
        try:
            verify_byom_attribution(byom_ctx["daemon"], [{
                "kind": "not.a.mapped.kind", "actor_ref": GOV_ACTOR,
                "event_id": "evt-synthetic", "causation_ref": "cause",
                "correlation_ref": "corr"}], sov)
        except Fail as refusal:
            ev.step("byom: the actor map is exhaustive by construction — "
                    "a synthetic event of an unmapped kind FAILS the "
                    "check instead of being skipped",
                    refusal=str(refusal))
        else:
            raise Fail("an unmapped event kind must fail --verify-trails")

        # -- kovee: its OWN events, its OWN provenance — never joined
        #    with the byom timeline.
        ktable = []
        for e in kovee_ctx["events"]:
            actor = e.get("actor_ref") or ""
            need(actor == KOVEE_ACTOR,
                 f"kovee event actor {actor!r} != {KOVEE_ACTOR!r}: {e}")
            need(e.get("correlation_ref") and e.get("occurred_at"),
                 f"kovee event without provenance: {e}")
            ktable.append({"type": e["type"], "actor_ref": actor})
            payload = e.get("payload") or {}
            if e["type"] == "dev.kovee.space.contribution-appended.v1":
                need(payload.get("author_actor_ref") == actor,
                     f"kovee contribution author drift: {e}")
        ev.blob("kovee-attribution.json", json.dumps(ktable, indent=2))
        ev.step("kovee: every event attributed to the owner principal "
                f"({KOVEE_ACTOR}); contribution payloads carry the same "
                "author_actor_ref; verified from kovee's events only",
                events=len(kovee_ctx["events"]))
        print(f"i0-trails: PASS ({ev.n} steps; evidence "
              f"{ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()
        if byom_ctx:
            byom_ctx["daemon"].cleanup()
        if kovee_ctx:
            kovee_ctx["daemon"].cleanup()


# ------------------------------------------------------- harness modes ----

# The real-harness modes drive the SAME complete I0 flow as --scripted,
# with every participant-side step performed by a REAL Claude Code /
# Codex session over the real byom-mcp and kovee-mcp stdio servers.
#
# The split is the sheet's, not a convenience:
#   - the AGENT's own steps are the harness's — membership_accept (the
#     candidate profile), then mandate_prepare, activity_open
#     (exploration), wake_intent_submit, pledge_propose, the PLEDGOR
#     seat's pledge_position, pledge_finalize, activity_open
#     (pledge_work), delivery_submit, and the kovee calls;
#   - the GOVERNANCE and HUMAN-SEAT steps stay with this driver on the
#     direct human channel, because they are the human's — the two
#     admissions, the mandate seat position and mandate_issue, the
#     endeavor chain and call_open, the BENEFICIARY seat's
#     pledge_position, and review_record.
#
# A governed flow interleaves by construction (the agent cannot proceed
# past an admission or a seat position it does not author), so the
# harness runs as successive invocations with the driver's steps in
# between. Every argument is fixed by the driver — the exact values
# --scripted sends — and every CLAIM is verified afterwards from byomd's
# and koveed's OWN event ledgers. Nothing the harness says is evidence.


def byom_candidate_server(d: ByomDaemon, offer_id: str) -> dict:
    return {"command": byom_mcp_bin(),
            "args": ["--profile", "candidate"],
            "env": {"BYOM_RUNTIME_DIR": str(d.run_dir),
                    "BYOM_CANDIDATE_TOKEN_FILE":
                        str(d.token_file(f"candidate-{offer_id}.token"))}}


def byom_participant_server(d: ByomDaemon, society: str) -> dict:
    return {"command": byom_mcp_bin(),
            "args": ["--profile", "participant"],
            "env": {"BYOM_RUNTIME_DIR": str(d.run_dir),
                    "BYOM_PARTICIPANT_TOKEN_FILE":
                        str(d.token_file(f"participant-{AGENT}.token")),
                    "BYOM_SOCIETY": society}}


def kovee_server(k: Koveed, project: str) -> dict:
    return {"command": kovee_mcp_bin(),
            "env": {"KOVEE_RUNTIME_DIR": str(k.run_dir),
                    "KOVEE_PROJECT": project}}


def harness_prompt(calls: list) -> str:
    """One session's instruction: the exact tool calls, in order, with
    the exact arguments. The harness is the ACTOR, not the author of the
    request bodies — the same bodies --scripted sends."""
    body = [
        "You are the agent participant of a byom Society. The MCP tools "
        "you have been given are your only surface onto it.",
        "",
        "Make the following tool calls, in this exact order. Copy every "
        "argument value VERBATIM — do not reformat, rename, shorten, "
        "invent or omit any field. Make no other tool call, and do not "
        "ask for confirmation.",
    ]
    for i, (_server, tool, args) in enumerate(calls, 1):
        body += ["", f"{i}. Call `{tool}` with exactly these arguments:",
                 json.dumps(args, indent=2)]
    body += ["", "When every call has returned, reply with the word DONE "
                 "followed by the identifiers the calls returned."]
    return "\n".join(body)


def harness_session(which: str, cli_path: str, ev: Evidence, n: int,
                    slug: str, calls: list, servers: dict,
                    workdir: Path) -> subprocess.CompletedProcess:
    """One REAL harness invocation. Identical tool schemas and zero
    server-side changes versus --scripted: only the caller differs."""
    prompt = harness_prompt(calls)
    allowed = sorted({f"mcp__{server}__{tool}" for server, tool, _ in calls})
    if which == "claude":
        config = ev.dir / f"session-{n:02d}-{slug}.mcp.json"
        config.write_text(json.dumps({"mcpServers": servers}, indent=1),
                          encoding="utf-8")
        argv = [cli_path, "-p", prompt,
                "--mcp-config", str(config), "--strict-mcp-config",
                "--allowedTools", ",".join(allowed)]
    else:
        overrides = []
        for name, spec in servers.items():
            key = name.replace("-", "_")
            overrides += ["-c", f"mcp_servers.{key}.command="
                                f"{json.dumps(spec['command'])}"]
            if spec.get("args"):
                overrides += ["-c", f"mcp_servers.{key}.args="
                                    f"{json.dumps(spec['args'])}"]
            for ek, evv in spec.get("env", {}).items():
                overrides += ["-c", f"mcp_servers.{key}.env.{ek}="
                                    f"{json.dumps(evv)}"]
        # Codex 0.145.0 has no per-tool allowlist (no equivalent of
        # Claude's --allowedTools; probing the shipped binary shows no
        # mcp_servers.<n>.{enabled_tools,auto_approve,trust} key exists),
        # so the grant is bounded structurally instead: with
        # --ignore-user-config the session's ONLY MCP servers are the ones
        # configured here, so "every tool" IS exactly our tool set.
        #
        # Both settings below are required, and it took isolating them to
        # see why: approval_policy="never" alone still fails, because an
        # MCP tool call crosses a process boundary that read-only and
        # workspace-write sandboxes deny — codex then reports the denial
        # as "user cancelled MCP tool call". danger-full-access is the
        # sandbox that permits the MCP transport; approval_policy covers
        # the approval prompt. Stated as explicit config rather than
        # --dangerously-bypass-approvals-and-sandbox (same effect, but
        # auditable and narrower in intent).
        #
        # Interactively the harness prompt remains the human trust
        # decision — that is the design (plan D7); this is only the
        # non-interactive gate's bounded stand-in.
        argv = [cli_path, "exec", "--skip-git-repo-check",
                "--ignore-user-config", "-s", "danger-full-access",
                "-c", 'approval_policy="never"', *overrides, prompt]
    started = time.time()
    # stdin MUST be closed: codex exec otherwise reads the inherited
    # stdin and treats its EOF as an interactive cancel, aborting the
    # in-flight MCP tool call ("user cancelled MCP tool call").
    session = subprocess.run(argv, capture_output=True, text=True,
                             stdin=subprocess.DEVNULL, cwd=str(workdir),
                             timeout=900)
    ev.blob(f"session-{n:02d}-{slug}.txt",
            f"$ {' '.join(argv)}\n--- exit {session.returncode} after "
            f"{time.time() - started:.1f}s\n--- allowed tools\n"
            f"{chr(10).join(allowed)}\n--- stdout\n{session.stdout}\n"
            f"--- stderr\n{session.stderr}")
    need(session.returncode == 0,
         f"{which} session {n:02d} ({slug}) failed "
         f"({session.returncode}): {session.stderr[-600:]}")
    return session


def kovee_branch_head(k: Koveed, project: str, branch: str) -> str:
    """The §10.3 head any authorized reader folds from kovee's events."""
    reply = k.expect_ok(kv_read("events_read", project,
                                {"source": project, "limit": 512}))
    entries = []
    for e in reply["result"]["events"]:
        if e["type"] != "dev.kovee.space.contribution-appended.v1":
            continue
        p = e.get("payload") or {}
        if p.get("origin_branch_id") == branch:
            entries.append((p["origin_branch_sequence"], p["content_digest"]))
    head = kovee_genesis_head(branch)
    for seq, cdigest in sorted(entries):
        head = kovee_next_head(head, seq, cdigest)
    return head


def harness_instructions(which: str) -> str:
    byom_mcp = byom_mcp_bin()
    kovee_mcp = kovee_mcp_bin()
    common = f"""\
Spawn the daemons first (isolated dirs), mint the offer, then register
the MCP servers with the harness. <run-dir>/<data-dir> are the byomd
dirs, <kovee-run-dir> the koveed runtime dir, <offer-id> the minted
MembershipOffer:

  byomd:  BYOM_DATA_DIR=<data-dir> BYOM_RUNTIME_DIR=<run-dir> {byomd_bin()}
  koveed: KOVEE_RUNTIME_DIR=<kovee-run-dir> {koveed_bin()} --data-dir <kovee-data-dir>
  # genesis + offer over the governance socket (the direct human channel),
  # e.g. via this scenario: python3 {HERE / 'run.py'} --scripted

The harness then drives the AGENT half of the I0 flow — membership_accept
(candidate profile), mandate_prepare, activity_open kind=exploration,
wake_intent_submit, pledge_propose, the pledgor seat's pledge_position,
pledge_finalize, activity_open kind=pledge_work, delivery_submit, and the
kovee calls — while the governance/human-seat steps (both admissions,
mandate_position + mandate_issue, the endeavor chain, call_open, the
beneficiary seat's pledge_position, review_record) stay on the direct
human channel. Every step is verified afterwards from byomd's and
koveed's own event ledgers.
"""
    if which == "claude":
        return common + f"""
  claude mcp add byom-candidate \\
    --env BYOM_RUNTIME_DIR=<run-dir> \\
    --env BYOM_CANDIDATE_TOKEN_FILE=<data-dir>/channels/candidate-<offer-id>.token \\
    -- {byom_mcp} --profile candidate
  # after participant_admit (the candidate channel closes at admission):
  claude mcp add byom \\
    --env BYOM_RUNTIME_DIR=<run-dir> \\
    --env BYOM_PARTICIPANT_TOKEN_FILE=<data-dir>/channels/participant-{AGENT}.token \\
    --env BYOM_SOCIETY=<society-id> \\
    -- {byom_mcp} --profile participant
  claude mcp add kovee \\
    --env KOVEE_RUNTIME_DIR=<kovee-run-dir> \\
    --env KOVEE_PROJECT=<project-id> \\
    -- {kovee_mcp}

Then drive each step (identical tool schemas, zero server-side changes
vs --scripted):

  claude -p "<the step's tool call and its exact arguments>" \\
    --mcp-config <session-config>.json --strict-mcp-config \\
    --allowedTools "mcp__byom__byom_activity_open"
"""
    return common + f"""
  # ~/.codex/config.toml (or `codex mcp add ...` where available):
  [mcp_servers.byom_candidate]
  command = "{byom_mcp}"
  args = ["--profile", "candidate"]
  env = {{ BYOM_RUNTIME_DIR = "<run-dir>", BYOM_CANDIDATE_TOKEN_FILE = "<data-dir>/channels/candidate-<offer-id>.token" }}

  [mcp_servers.byom]
  command = "{byom_mcp}"
  args = ["--profile", "participant"]
  env = {{ BYOM_RUNTIME_DIR = "<run-dir>", BYOM_PARTICIPANT_TOKEN_FILE = "<data-dir>/channels/participant-{AGENT}.token", BYOM_SOCIETY = "<society-id>" }}

  [mcp_servers.kovee]
  command = "{kovee_mcp}"
  env = {{ KOVEE_RUNTIME_DIR = "<kovee-run-dir>", KOVEE_PROJECT = "<project-id>" }}

Then drive each step:

  codex exec --skip-git-repo-check --ignore-user-config \\
    -s danger-full-access -c approval_policy="never" \\
    "<the step's tool call and its exact arguments>"
"""


def mode_harness(which: str) -> int:
    test_id = f"i0-flow-{which}"
    instructions = harness_instructions(which)
    if os.environ.get("I0_REAL_HARNESS") != "1":
        print(f"{test_id}: SKIP (env-gated; set I0_REAL_HARNESS=1 to run "
              "a real harness session). Setup commands:\n")
        print(instructions)
        return 2
    harness_cli = shutil.which(which)
    if harness_cli is None:
        print(f"{test_id}: SKIP — I0_REAL_HARNESS=1 but no `{which}` CLI "
              "on PATH. Setup commands:\n")
        print(instructions)
        return 2

    ev = Evidence(test_id)
    print(f"{test_id}: a real {which} session drives the COMPLETE I0 "
          "agent half on both daemons; every claim is verified from "
          "byomd's and koveed's own event ledgers")
    ev.blob("setup-instructions.txt", instructions)
    workdir = Path(tempfile.mkdtemp(prefix=f"i0-harness-{which}-cwd-"))
    d = ByomDaemon(f"h-{which}")
    k = Koveed(f"h-{which}")
    tag = f"h{which}"
    sessions = {"n": 0}
    genesis = ""
    agent_actor = f"participant:{AGENT}"
    sov = ""

    def drive(slug: str, calls: list, servers: dict):
        sessions["n"] += 1
        return harness_session(which, harness_cli, ev, sessions["n"], slug,
                               calls, servers, workdir)

    def events(kind: str | None = None) -> list:
        rows = timeline(d, genesis)
        return [e for e in rows if kind is None or e["kind"] == kind]

    def payload_of(e: dict) -> dict:
        return d.expect_ok("projection", {
            "version": "0.2", "op": "event_payload",
            "event_id": e["event_id"]})["result"]["payload"]

    def authored(kind: str, expect: str, count: int = 1) -> dict:
        """Exactly `count` events of `kind` in byomd's OWN ledger, each
        authored by `expect`; the last one is returned."""
        rows = events(kind)
        need(len(rows) == count,
             f"byomd's ledger must hold exactly {count} {kind} "
             f"event(s), it holds {len(rows)}")
        for r in rows:
            need(r["actor_ref"] == expect,
                 f"{kind} authored by {r['actor_ref']!r}, expected "
                 f"{expect!r}")
        return rows[-1]

    try:
        inc = d.incarnation()

        # -- driver [governance, direct human channel]: atomic genesis
        #    and the membership offer naming the proposed
        #    attached_harness ManifestationRevision.
        prepared = d.expect_ok("governance", {
            "version": "0.2", "op": "society_prepare",
            "meta": meta(inc, f"{tag}-prep"),
            "home_authority_ref": "auth-home-1",
            "proposed_charter_ref": "charter-draft-1",
            "proposed_charter_digest": digest(0xA1),
            "classification_binding_ref": "class-bind-1",
            "classification_binding_digest": digest(0xA2)})
        society = prepared["result"]["society_id"]
        booted = d.expect_ok("governance", {
            "version": "0.2", "op": "society_bootstrap",
            "meta": meta(inc, f"{tag}-boot", 1),
            "society_id": society,
            "preparation_ref": prepared["result"]["preparation_ref"],
            "subject_digest": prepared["result"]["subject_digest"]})
        genesis = booted["source_cursor"]
        subject = digest(0xB1)
        offered = d.expect_ok("governance", {
            "version": "0.2", "op": "membership_offer",
            "meta": meta(inc, f"{tag}-offer"),
            "participant_ref": AGENT,
            "proposed_standing_ref": "standing-proposal-1",
            "subject_digest": subject,
            "offered_by_decision_ref": f"dec-society-{society}",
            "expires_at": FAR_FUTURE})
        offer_id = offered["result"]["offer_id"]
        manifestation = authored("manifestation.proposed", GOV_ACTOR)
        need(payload_of(manifestation).get("kind") == "attached_harness",
             "the offer must propose an attached_harness Manifestation")
        manifestation_ref = manifestation["object_ref"]
        ev.step("driver [governance, direct human channel]: atomic "
                "genesis + membership_offer — the proposed "
                "attached_harness ManifestationRevision and the "
                "candidate channel credential exist",
                society_id=society, offer_id=offer_id,
                manifestation_ref=manifestation_ref)

        # -- driver [kovee CLI]: the space and the human's question, so
        #    the agent's later append CASes a non-genesis branch head.
        env_cli = {"KOVEE_RUNTIME_DIR": str(k.run_dir)}
        init = cli([kovee_cli_bin(), "init"], env_cli, ev,
                   "kovee-cli-init.txt")
        match = re.search(r"project:\s+(\S+)", init.stdout)
        need(match, f"kovee init printed no project: {init.stdout}")
        project = match.group(1)
        created = cli([kovee_cli_bin(), "space", "create", "--project",
                       project, "--title", f"I0 harness ({which})"],
                      env_cli, ev, "kovee-cli-space-create.txt")
        space_result = json.loads(created.stdout)
        space = space_result["space_id"]
        branch = space_result["main_branch_id"]
        cli([kovee_cli_bin(), "space", "contribute", "--project", project,
             "--space", space, "--kind", "question", "--text",
             "What does the attached harness owe the Society?"],
            env_cli, ev, "kovee-cli-question.txt")
        ev.step("driver [kovee CLI]: init, space create, the human's "
                "question contribution (branch sequence 1)",
                project_id=project, space_id=space, main_branch_id=branch)

        # == 1. THE AGENT accepts its own offer [candidate profile].
        cand_servers = {"byom-candidate": byom_candidate_server(d, offer_id)}
        drive("membership-accept", [
            ("byom-candidate", "byom_membership_accept",
             {"offer_ref": offer_id, "subject_digest": subject})],
            cand_servers)
        accepted = events("membership.accepted")
        need(len(accepted) == 1,
             f"the real {which} session must accept exactly once, "
             f"byomd's ledger holds {len(accepted)}")
        need(accepted[0]["actor_ref"].startswith("candidate:"),
             f"acceptance actor: {accepted[0]}")
        acceptance = payload_of(accepted[0])["acceptance_id"]
        ev.step(f"{which} session 01 drove byom_membership_accept over "
                "the byom-mcp CANDIDATE profile — VERIFIED from byomd's "
                "events_read: exactly one membership.accepted, authored "
                "by the candidate channel",
                actor_ref=accepted[0]["actor_ref"],
                acceptance_id=acceptance)

        # -- driver [governance]: the two admissions -> active Standing.
        d.expect_ok("governance", {
            "version": "0.2", "op": "participant_admit",
            "meta": meta(inc, f"{tag}-admit", 2),
            "offer_ref": offer_id,
            "membership_acceptance_ref": acceptance,
            "admitted_by_decision_ref": f"dec-offer-{offer_id}",
            "admission_subject_digest": subject})
        d.expect_ok("governance", {
            "version": "0.2", "op": "manifestation_admit",
            "meta": meta(inc, f"{tag}-manif", 1),
            "manifestation_ref": manifestation_ref,
            "admitted_by_decision_ref": f"dec-manif-{manifestation_ref}"})
        authored("membership.admitted", GOV_ACTOR)
        authored("manifestation.admitted", GOV_ACTOR)
        authored("channel.converted", GOV_ACTOR)
        need(d.token_file(f"participant-{AGENT}.token").exists(),
             "the participant channel credential must be minted at "
             "admission")
        sov = sovereign_id(d, society)
        part_servers = {"byom": byom_participant_server(d, society)}
        ev.step("driver [governance, direct human channel]: "
                "participant_admit + manifestation_admit — Standing "
                "active, candidate channel closed, participant channel "
                "minted (verified from byomd's events)",
                participant=AGENT, sovereign=sov)

        # == 2. THE AGENT prepares its own mandate.
        drive("mandate-prepare", [
            ("byom", "byom_mandate_prepare", {
                "grantee_participant_ref": AGENT,
                "purpose_ref": "purpose-explore-1",
                "allowed_operations": ["activity_open",
                                       "continuation_write",
                                       "wake_intent_submit"],
                "resource_selectors": ["res-repo-1"],
                "data_class_selectors": ["class-public"],
                "destination_selectors": [],
                "budget_ceiling_set_ref": "budget-mandate-1",
                "concurrency_ceiling": 2,
                "delegation": {"allowed": False, "max_depth": 0,
                               "max_children": 0, "grantee_selectors": []},
                "expires_at": FAR_FUTURE})],
            part_servers)
        mandate = authored("mandate.prepared", agent_actor)["object_ref"]
        mandate_row = d.store_row("mandates", "mandate_id", mandate)
        mandate_subject = json.loads(mandate_row["subject_digest"])
        seat = json.loads(mandate_row["required_seat_refs"])[0]["seat_ref"]
        ev.step(f"{which} session 02 drove byom_mandate_prepare over the "
                "byom-mcp PARTICIPANT profile — VERIFIED from byomd's "
                f"events_read: mandate.prepared authored by {agent_actor}",
                mandate_id=mandate, required_seat_ref=seat)

        # -- driver [governance]: the human seat's position + issue.
        d.expect_ok("governance", {
            "version": "0.2", "op": "mandate_position",
            "meta": meta(inc, f"{tag}-mpos"),
            "proposal_ref": mandate, "proposal_revision": 1,
            "subject_digest": mandate_subject,
            "seat_ref": seat, "value": "assent"})
        d.expect_ok("governance", {
            "version": "0.2", "op": "mandate_issue",
            "meta": meta(inc, f"{tag}-missue", 1),
            "mandate_id": mandate, "subject_digest": mandate_subject})
        authored("mandate.position_recorded", GOV_ACTOR)
        authored("mandate.issued", GOV_ACTOR)
        authored("budget.reserved", GOV_ACTOR)
        ev.step("driver [governance, direct human channel]: "
                "mandate_position (human seat, fresh challenge) + "
                "mandate_issue with budget reservation",
                mandate_id=mandate)

        # == 3. THE AGENT opens the exploration activity under it.
        drive("activity-open-exploration", [
            ("byom", "byom_activity_open", {
                "kind": "exploration", "purpose_ref": "purpose-explore-1",
                "purpose_digest": digest(0xC0),
                "mandate_refs": [mandate],
                "budget_account_set_ref": "budget-mandate-1"})],
            part_servers)
        opened = authored("activity.opened", agent_actor)
        exploration = opened["object_ref"]
        opened_payload = payload_of(opened)
        need(opened_payload.get("kind") == "exploration"
             and opened_payload.get("mandate_refs") == [mandate]
             and opened_payload.get("state") == "ready",
             f"exploration activity: {opened_payload}")
        ev.step(f"{which} session 03 drove byom_activity_open "
                "kind=exploration UNDER THE MANDATE — VERIFIED from "
                "byomd's events_read: activity.opened authored by "
                f"{agent_actor}, kind=exploration, mandate_refs=[{mandate}]",
                activity_stream=exploration)

        # == 4. THE AGENT submits the wake intent (left pending in I0).
        drive("wake-intent-submit", [
            ("byom", "byom_wake_intent_submit", {
                "activity_stream_ref": exploration, "generation": 1,
                "origin": "direct_participant",
                "exact_cause_ref": "cause-followup-1",
                "exact_cause_digest": digest(0xC2),
                "purpose_ref": "purpose-explore-1",
                "stable_wake_key": f"wake-{tag}",
                "expires_at": FAR_FUTURE})],
            part_servers)
        wake = authored("wake-intent.submitted", agent_actor)
        need(payload_of(wake).get("state") == "submitted",
             "the wake intent must be recorded submitted")
        need(not [e for e in events()
                  if e["kind"].startswith("wake-intent.")
                  and e["kind"] != "wake-intent.submitted"],
             "the wake intent must stay pending — no activation in I0")
        ev.step(f"{which} session 04 drove byom_wake_intent_submit — "
                "VERIFIED from byomd's events_read: exactly one "
                "wake-intent.submitted authored by the agent, still "
                "pending (no activation event of any kind)",
                wake_intent=wake["object_ref"])

        # -- driver [human sovereign, direct human channel]: the
        #    endeavor chain and the call the agent will pledge into.
        eprop = d.expect_ok("participant", {
            "version": "0.2", "op": "endeavor_propose",
            "meta": meta(inc, f"{tag}-eprop"),
            "purpose_ref": "purpose-improve-1",
            "purpose_digest": digest(0xD0),
            "sponsor_participant_refs": [sov],
            "governance_rule_set_ref": "rules-endeavor-1",
            "outcome_schema_refs": ["schema-change-set-1"],
            "acceptance_rule_ref": "rule-accept-1",
            "classification_join_ref": "class-join-1",
            "budget_account_set_ref": f"budget-endeavor-{tag}"})
        endeavor = eprop["result"]["endeavor_id"]
        d.expect_ok("participant", {
            "version": "0.2", "op": "endeavor_position",
            "meta": meta(inc, f"{tag}-epos"),
            "proposal_ref": endeavor, "proposal_revision": 1,
            "subject_digest": eprop["result"]["subject_digest"],
            "seat_ref": eprop["result"]["required_seat_refs"][0],
            "value": "assent"})
        d.expect_ok("participant", {
            "version": "0.2", "op": "endeavor_finalize",
            "meta": meta(inc, f"{tag}-efin", 1),
            "endeavor_id": endeavor,
            "subject_digest": eprop["result"]["subject_digest"]})
        call_opened = d.expect_ok("participant", {
            "version": "0.2", "op": "call_open",
            "meta": meta(inc, f"{tag}-call"),
            "endeavor_id": endeavor,
            "requested_outcome_schema_refs": ["schema-change-set-1"],
            "acceptance_criteria_refs": ["criteria-review-1"],
            "evidence_requirements": []})
        call_id = call_opened["result"]["call_id"]
        human_actor = f"participant:{sov}"
        authored("endeavor.proposed", human_actor)
        authored("endeavor.finalized", human_actor)
        authored("call.opened", human_actor)
        ev.step("driver [human sovereign, direct human channel]: "
                "endeavor_propose/position/finalize + call_open",
                endeavor_id=endeavor, call_id=call_id)

        # == 5. THE AGENT proposes the pledge into that call.
        drive("pledge-propose", [
            ("byom", "byom_pledge_propose", {
                "endeavor_id": endeavor, "call_ref": call_id,
                "proposed_pledgor_ref": AGENT, "beneficiary_ref": sov,
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
                "dependency_refs": []})],
            part_servers)
        proposal = authored("pledge.proposed", agent_actor)["object_ref"]
        prop_row = d.store_row("pledge_proposals", "proposal_id", proposal)
        terms = json.loads(prop_row["terms_digest"])
        slots = {s["kind"]: s["seat_ref"]
                 for s in json.loads(prop_row["required_slots"])["seats"]}
        ev.step(f"{which} session 05 drove byom_pledge_propose — "
                "VERIFIED from byomd's events_read: pledge.proposed "
                f"authored by {agent_actor}; the pledgor and beneficiary "
                "seats were minted",
                proposal_id=proposal, required_slots=sorted(slots))

        # -- driver [human sovereign]: the BENEFICIARY seat's position.
        d.expect_ok("participant", {
            "version": "0.2", "op": "pledge_position",
            "meta": meta(inc, f"{tag}-ppos-sov"),
            "proposal_ref": proposal, "proposal_revision": 1,
            "subject_digest": terms,
            "seat_ref": slots["beneficiary_assent"],
            "value": "assent", "assent_mode": "direct_participant"})
        ev.step("driver [human sovereign, direct human channel]: "
                "pledge_position for the BENEFICIARY seat",
                seat_ref=slots["beneficiary_assent"])

        # == 6. THE AGENT positions its own seat and finalizes.
        drive("pledge-position-finalize", [
            ("byom", "byom_pledge_position", {
                "proposal_ref": proposal, "proposal_revision": 1,
                "subject_digest": terms,
                "seat_ref": slots["pledgor_assent"],
                "value": "assent", "assent_mode": "direct_participant"}),
            ("byom", "byom_pledge_finalize", {
                "proposal_ref": proposal, "proposal_revision": 1,
                "subject_digest": terms})],
            part_servers)
        position_events = events("pledge.position_recorded")
        need(len(position_events) == 2,
             f"exactly two pledge positions (one per required seat), "
             f"byomd's ledger holds {len(position_events)}")
        positions = {e["actor_ref"]: payload_of(e)["seat_ref"]
                     for e in position_events}
        need(positions.get(agent_actor) == slots["pledgor_assent"],
             f"the pledgor seat must be authored by the agent: {positions}")
        need(positions.get(human_actor) == slots["beneficiary_assent"],
             f"the beneficiary seat must be the human's: {positions}")
        pledge = authored("pledge.committed", agent_actor)["object_ref"]
        ev.step(f"{which} session 06 drove byom_pledge_position (PLEDGOR "
                "seat) + byom_pledge_finalize — VERIFIED from byomd's "
                "events_read: the two seats are two distinct actors and "
                f"pledge.committed is authored by {agent_actor}",
                pledge_id=pledge, seat_actors=positions)

        # == 7. THE AGENT opens the bound pledge_work stream.
        drive("activity-open-pledge-work", [
            ("byom", "byom_activity_open", {
                "kind": "pledge_work", "purpose_ref": "purpose-improve-1",
                "purpose_digest": digest(0xD4),
                "pledge_binding": {"pledge_id": pledge,
                                   "pledge_revision": 1,
                                   "terms_digest": terms},
                "mandate_refs": [],
                "budget_account_set_ref": f"budget-endeavor-{tag}"})],
            part_servers)
        work_stream = authored("activity.opened", agent_actor,
                               count=2)["object_ref"]
        authored("pledge.underway", agent_actor)
        ev.step(f"{which} session 07 drove byom_activity_open "
                "kind=pledge_work bound to the pledge — VERIFIED from "
                "byomd's events_read: the second activity.opened and "
                f"pledge.underway are authored by {agent_actor}",
                work_stream=work_stream)

        # == 8. THE AGENT submits the deterministic delivery.
        drive("delivery-submit", [
            ("byom", "byom_delivery_submit", {
                "pledge_id": pledge, "pledge_revision": 2,
                "terms_digest": terms,
                "output_refs": ["change-set-1"],
                "evidence_refs": ["attest-complete-readable-source-1"],
                "activity_stream_ref": work_stream})],
            part_servers)
        delivered = authored("delivery.submitted", agent_actor)
        delivery = delivered["object_ref"]
        delivery_row = d.store_row("deliveries", "delivery_id", delivery)
        ev.step(f"{which} session 08 drove byom_delivery_submit — "
                "VERIFIED from byomd's events_read: delivery.submitted "
                f"authored by {agent_actor} against the pledge",
                delivery_id=delivery,
                classification=payload_of(delivered).get("classification"))

        # -- driver [human sovereign]: review_record closes the pledge.
        #    The review CASes the pledge's CURRENT revision (the one the
        #    delivery advanced it to), read from byomd's own store.
        pledge_row = d.store_row("pledges", "pledge_id", pledge)
        reviewed = d.expect_ok("participant", {
            "version": "0.2", "op": "review_record",
            "meta": meta(inc, f"{tag}-review"),
            "pledge_id": pledge,
            "pledge_revision": int(pledge_row["revision"]),
            "delivery_id": delivery,
            "reviewed_subject_digest":
                json.loads(delivery_row["subject_digest"]),
            "outcome": "fulfilled",
            "decision_or_mandate_use_ref": "dec-review-1"})
        need(reviewed["result"]["pledge_state"] == "fulfilled",
             f"review: {reviewed}")
        authored("review.recorded", human_actor)
        snap = d.expect_ok("projection", {
            "version": "0.2", "op": "snapshot_get",
            "society_id": society, "kinds": ["pledges"]})
        pledges = snap["result"]["pledges"]
        need(len(pledges) == 1 and pledges[0]["state"] == "fulfilled",
             f"exactly one fulfilled pledge: {pledges}")
        ev.step("driver [human sovereign, direct human channel]: "
                "review_record — the single pledge is fulfilled "
                "(verified from byomd's snapshot + events)",
                review_id=reviewed["result"]["review_id"])

        # == 9. THE AGENT touches kovee through kovee-mcp: read the
        #       space, then append a contribution CASed against the head
        #       this driver folded from koveed's own events.
        head = kovee_branch_head(k, project, branch)
        agent_text = (f"The {which} harness session appended this through "
                      "kovee-mcp: in I0 the two stacks stay separate.")
        drive("kovee-contribution", [
            ("kovee", "kovee_space_show", {"space_id": space}),
            ("kovee", "kovee_contribution_append", {
                "space_id": space, "branch_id": branch,
                "expected_head_digest": head, "kind": "utterance",
                "body_parts": [{"media_type": "text/plain",
                                "text": agent_text}]})],
            {"kovee": kovee_server(k, project)})
        kev = k.expect_ok(kv_read("events_read", project,
                                  {"source": project,
                                   "limit": 512}))["result"]["events"]
        appends = [e for e in kev
                   if e["type"] == "dev.kovee.space.contribution-appended.v1"]
        need(len(appends) == 2,
             f"koveed's ledger must hold the human's question and the "
             f"harness's contribution, it holds {len(appends)}")
        agent_append = appends[-1]
        agent_payload = agent_append.get("payload") or {}
        need(agent_payload.get("origin_branch_id") == branch
             and agent_payload.get("origin_branch_sequence") == 2,
             f"the harness contribution must extend the branch: "
             f"{agent_payload}")
        need(agent_append.get("actor_ref") == KOVEE_ACTOR,
             f"kovee actor: {agent_append}")
        contribution_id = agent_payload.get("contribution_id")
        need(contribution_id, f"no contribution id in {agent_payload}")
        shown = k.expect_ok(kv_read("contribution_show", project,
                                    {"contribution_id": contribution_id}))
        body = json.dumps(shown["result"])
        need(agent_text in body,
             "koveed's own record of the contribution must carry the "
             "exact body the session was told to append")
        need(kovee_next_head(head, 2, agent_payload["content_digest"])
             == kovee_branch_head(k, project, branch),
             "the §10.3 head chain must fold to the same head")
        ev.step(f"{which} session 09 drove kovee_space_show + "
                "kovee_contribution_append through kovee-mcp — VERIFIED "
                "from koveed's own events_read and contribution_show: "
                "branch sequence 2, exact body, head chain folds — the "
                "SECOND daemon is genuinely exercised",
                contribution_id=contribution_id,
                expected_head_digest=head)

        # -- the whole flow, per source. byom: order, exclusions, and
        #    the EXHAUSTIVE actor map (the same check --verify-trails
        #    applies to the scripted run).
        rows = events()
        kinds = [e["kind"] for e in rows]
        assert_ordered(kinds, BYOM_EXPECTED_ORDER, f"byom/{which}")
        forbidden = [kk for kk in kinds
                     if kk.startswith(("episode.", "placement.",
                                       "activation."))
                     or kk == "wake-intent.activated"]
        need(not forbidden,
             f"I0 excludes activation/placement/episodes, saw {forbidden}")
        table = verify_byom_attribution(d, rows, sov)
        ev.blob("byom-attribution.json", json.dumps(table, indent=2))
        ev.blob("byom-timeline.json", json.dumps(kinds, indent=1))
        types = [e["type"] for e in kev]
        assert_ordered(types, ["dev.kovee.project.created.v1",
                               "dev.kovee.space.created.v1",
                               "dev.kovee.space.contribution-appended.v1",
                               "dev.kovee.space.contribution-appended.v1"],
                       f"kovee/{which}")
        for i, e in enumerate(kev):
            need(e.get("project_sequence") == i + 1,
                 f"kovee project sequences not dense at {i}: {e}")
            need(e.get("actor_ref"), f"kovee event without actor: {e}")
        ev.blob("kovee-timeline.json", json.dumps(types, indent=1))
        ev.step(f"per-source trails for the {which} run: byom "
                "events_read in the sheet's order with EVERY kind "
                "checked against the exhaustive actor map, no "
                "activation/placement/episode; kovee's own events dense "
                "and attributed — asserted separately, never merged",
                byom_events=len(rows), byom_kinds=len(set(kinds)),
                kovee_events=len(kev),
                harness_sessions=sessions["n"])
        print(f"{test_id}: PASS ({sessions['n']} real {which} sessions, "
              f"{ev.n} steps; evidence {ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()
        d.cleanup()
        k.cleanup()
        shutil.rmtree(workdir, ignore_errors=True)
# --------------------------------------------------------- cargo suites ----

CARGO_SUITES = [
    ("i0-negative", "b1_onboarding_negative"),
    ("i0-privacy", "b1_privacy_access"),
    ("i0-classification", "b1_classification"),
]


def mode_cargo_suites() -> int:
    for test_id, suite in CARGO_SUITES:
        ev = Evidence(test_id)
        print(f"{test_id}: cargo test -p byomd --test {suite}")
        proc = subprocess.run(
            ["cargo", "test", "-q", "-p", "byomd", "--test", suite,
             "--locked"],
            cwd=REPO, capture_output=True, text=True)
        ev.blob("cargo-test.txt",
                f"--- exit {proc.returncode}\n--- stdout\n{proc.stdout}"
                f"--- stderr\n{proc.stderr}")
        ev.close()
        if proc.returncode != 0:
            print(proc.stdout)
            print(proc.stderr, file=sys.stderr)
            print(f"{test_id}: FAIL")
            return 1
        summary = [l for l in proc.stdout.splitlines()
                   if "test result" in l]
        print(f"{test_id}: PASS ({'; '.join(summary) or 'ok'})")
    return 0


# ---------------------------------------------------------------- main ----

def main(argv: list) -> int:
    args = argv[1:]
    try:
        if args == ["--scripted"]:
            return mode_scripted()
        if args == ["--crash-matrix"]:
            return mode_crash_matrix()
        if args == ["--verify-trails"]:
            return mode_verify_trails()
        if len(args) == 2 and args[0] == "--harness" \
                and args[1] in ("claude", "codex"):
            return mode_harness(args[1])
        if args == ["--all-checks"]:
            for mode in (mode_scripted, mode_crash_matrix,
                         mode_verify_trails, mode_cargo_suites):
                code = mode()
                if code != 0:
                    return code
            print("i0: all checks PASS (scripted, crash-matrix, trails, "
                  "negative, privacy, classification)")
            return 0
    except Fail as e:
        print(f"FAIL  {e}")
        return 1
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
