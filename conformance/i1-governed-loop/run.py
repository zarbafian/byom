#!/usr/bin/env python3
"""I1 governed loop with real intelligence (the integration gate; plan §8 I1).

Both stacks live side by side and the loop crosses BOTH of them: byomd
(this repo) owns every authority record, koveed (../kovee) owns
deliberation, placement and the disclosed metered model broker.

    python3 run.py --scripted        # i1-flow-scripted (gates CI; NO model call)
    python3 run.py --crash-matrix    # i1-crash (both daemons + the broker chain)
    python3 run.py --real-model      # i1-real-model (I1_REAL_MODEL=1; REAL providers)
    python3 run.py --harness claude  # i1-flow-claude (I1_REAL_HARNESS=1)
    python3 run.py --harness codex   # i1-flow-codex  (I1_REAL_HARNESS=1)
    python3 run.py --all-checks      # scripted + crash-matrix (the CI pair)

What the scripted gate drives, in order (every step names its owner):

    kovee governance enable   the D10 GREENFIELD saga, live: two inert
                              bindings, then the owner CAS none -> byom
    space + question          kovee's own deliberation records
    attention notice          byom's narrow attention channel: NOTIFICATION
                              IS NOT A WAKE — no admission, no allocation,
                              no episode from the notice alone
    wake_intent_submit        the PARTICIPANT's own wake, over byom-mcp
    episode_request           byom's kernel stages 2 and 3 (admission +
                              allocation) — BEFORE any placement (L25/A8)
    place / placement_admit   Kovee's PlacementBinding, byom's adapter
    episode_claim/start       the lease, under DUAL fences
    kovee_endeavor_form       the formation saga: exactly one Endeavor
    call + pledge             the full seat sequence
    act_intent_* + broker     the model_egress act chain to a ONE-SHOT
                              permit, then Kovee's broker: prepared before
                              any dispatch, usage metered back to byom, and
                              BYOM (not kovee) settles

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
  - runtime adapters (attention notice, placement_admit, claim/start,
    usage_report, execution_permit_consume): byomd's runtime socket under
    the subject-scoped workload tokens byomd itself published;
  - kovee: the kovee CLI and socket (init, space, question,
    governance_enable, formation saga, invocation, reads) and — for the
    episode pipeline and the broker, which kovee exposes as library API
    only — `kovee-driver/`, a binary that links kovee's own crates and
    calls exactly the functions kovee's K2 suites call.

Evidence lands in evidence/<test-id>/. Every assertion is made from the
OWNING daemon's own records: byom facts from byomd's events/store, kovee
facts from koveed's events/store — per source, never merged.

Honest residuals of THIS gate (each one is also stated in the evidence):

  - kovee's `kovee-attention` crate is still a stub, so no kovee-side
    AttentionContract sender exists. The notice is delivered on byom's
    narrow attention runtime channel, carrying kovee's OWN committed event
    id — the record, the surface and every refusal are byom's, but the
    caller is this scenario, not a kovee subsystem.
  - the eligible arm of a notice (an ADOPTED ActivationPolicy) is covered
    by byomd's `b3_attention` suite; this gate asserts the no-effect arm
    and the participant's own four-stage activation.
  - the Manifestation the offer proposes is `attached_harness` (byom's
    offer shape proposes exactly that today); the hosted Episode runs
    under it. A distinct hosted-Manifestation kind is not exercised.
  - `--real-model` uses kovee's real TLS transport, but the exercised call
    is the only thing claimed: nothing here prevents a bypass of the
    broker. That is K4.
  - `Continuation` resume across Manifestations and the deliberately
    ambiguous effect walked through EOA -> disposition (plan §8 I1) are
    NOT part of this gate; byomd's `b3_effects`/`b3_recovery` and kovee's
    `k2_*` suites hold them, and I1 will absorb them when the runner grows
    a yield/resume cell.

Exit codes: 0 green, 1 failure, 2 honest skip (ungated mode).
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


def driver_bin() -> str:
    """The kovee-side driver: kovee's own crates, linked into one binary
    the scenario can call. Built in its own workspace so it never enters
    byom's lockfile or lints."""
    path = _target_dir(DRIVER_DIR) / "debug" / "i1-kovee-driver"
    # ALWAYS rebuild: reusing an existing binary silently mixes kovee
    # revisions, so a driver compiled against an older kovee could pass a
    # gate the current one fails (cargo is incremental, so this is cheap).
    subprocess.check_call(["cargo", "build", "-q"], cwd=DRIVER_DIR)
    need(path.exists(), f"driver missing after build: {path}")
    return str(path)


# ------------------------------------------------------------ evidence ----

class Evidence:
    """Per-test-id evidence: numbered step lines on stdout, a steps.jsonl
    transcript, and named blobs, under evidence/<test-id>/."""

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


class ByomDaemon:
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

    def kill(self):
        if self.proc is not None:
            self.proc.kill()
            self.proc.wait()
            self.proc = None

    def wait_exit(self):
        if self.proc is not None:
            self.proc.wait()
            self.proc = None

    def restart(self, env: dict | None = None):
        self.kill()
        self.start(env)

    def cleanup(self):
        self.kill()
        shutil.rmtree(self.data_dir, ignore_errors=True)
        shutil.rmtree(self.run_dir, ignore_errors=True)


class Koveed:
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

    def kill(self):
        if self.proc is not None:
            self.proc.kill()
            self.proc.wait()
            self.proc = None

    def wait_exit(self):
        if self.proc is not None:
            self.proc.wait()
            self.proc = None

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


def portable_of(text: str) -> dict:
    return {"class": "portable_public", "algorithm": "sha-256",
            "value_hex": hashlib.sha256(text.encode()).hexdigest()}


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
        need(not is_error, f"{self.tag}: {name}: {text}")
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

    def open(self) -> Mcp:
        self.sessions += 1
        return Mcp([byom_mcp_bin(), "--profile", "participant"],
                   {"BYOM_RUNTIME_DIR": str(self.byom.run_dir),
                    "BYOM_PARTICIPANT_TOKEN_FILE":
                        str(self.byom.token_file(
                            f"participant-{AGENT}.token")),
                    "BYOM_SOCIETY": self.society},
                   self.ev, "byom-mcp[participant]")

    def one(self, tool: str, arguments: dict, frames: str | None = None):
        mcp = self.open()
        try:
            return mcp.call_ok(tool, arguments)
        finally:
            mcp.close(frames)


def agent_socket_call(byom: ByomDaemon, request: dict) -> dict:
    """One agent-channel call over byomd's participant socket, made by a
    SHORT-LIVED child of this script.

    byom-mcp exposes 36 participant tools but not every participant
    operation (`activation_policy_adopt` has no tool binding yet), and a
    channel is held by one live process — so the call runs in a child that
    claims, calls, and exits, leaving the channel free for the next
    holder. The child is this same file (`--_agent-call`)."""
    proc = subprocess.run(
        [sys.executable, str(HERE / "run.py"), "--_agent-call",
         str(byom.run_dir),
         str(byom.token_file(f"participant-{AGENT}.token"))],
        input=json.dumps(request), capture_output=True, text=True)
    need(proc.returncode == 0,
         f"agent-call child failed ({proc.returncode}): {proc.stderr}")
    return json.loads(proc.stdout)


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

    def base(self) -> dict:
        return {"store": str(self.kovee.store_path()),
                "realm": REALM,
                "byom_run_dir": str(self.byom.run_dir),
                "byom_channels_dir": str(self.byom.channels_dir())}

    def run(self, command: str, args: dict,
            expect_ok: bool = True) -> tuple[dict, int]:
        self.calls += 1
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
    }
    prefix = {
        # the workload identity that holds the lease, and the candidate
        # channel that closes at admission
        "episode-lease.claimed": "runtime:",
        "episode.running": "runtime:",
        "episode.completed": "runtime:",
        "pledge.underway": "participant:",
        "membership.accepted": "candidate:",
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


def attention_notice(byom: ByomDaemon, stream: str, event: dict, key: str,
                     generation: int = 1) -> dict:
    """kovee Attention may NOTIFY byom's adapter of an admitted exact
    event; byom alone decides whether a Participant's WakeIntent and
    ActivityStream permit a new Episode (byom §16.4, family contract L25).

    The notice rides byom's narrow attention runtime channel under the
    token byomd published for this exact ActivityStream generation, and it
    carries kovee's OWN committed event id. kovee ships no attention
    sender yet (its `kovee-attention` crate is a stub), so the scenario
    delivers the notice as that adapter would — the record, the surface
    and the refusals are byom's own."""
    token = byom.read_token(f"runtime-attention-{stream}.token")
    return byom.expect_ok("runtime", {
        "version": "0.2", "op": "attention_notice_record",
        "meta": meta(byom.incarnation(), key),
        "source_protocol": "kovee",
        "source_endpoint_ref": "kovee-endpoint-local",
        "source_event_ref": event["event_id"],
        "source_event_digest": portable_of(event["event_id"]),
        "activity_stream_ref": stream,
        "generation": generation,
        "stable_notice_key": f"notice-{key}"}, token)


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
            "none -> byom at the expected revision; exact retry returns "
            "the identical binding (crash-safe), overlap impossible",
            binding_ref=enabled["binding"]["binding_ref"],
            governance_owner=enabled["owner_binding"]["governance_owner"],
            owner_revision=enabled["owner_binding"]["revision"],
            state=enabled["state"])

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
    notice = attention_notice(byom, stream, ctx["event"], f"{tag}-n1")
    r = notice["result"]
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
    replayed = attention_notice(byom, stream, ctx["event"], f"{tag}-n1")
    need(replayed == notice, "the exact retry replays byte-identically")
    need(byom.count("SELECT COUNT(*) FROM attention_notices") == 1,
         "a replay commits no second notice")
    ev.step("byom: attention_notice_record on the narrow attention runtime "
            "channel, carrying KOVEE's own committed event id — "
            "eligibility_effect=no_effect, created.{wake_intent,"
            "activation_admission,resource_allocation,episode} all false, "
            "and byom's four activation tables are still EMPTY: "
            "NOTIFICATION IS NOT A WAKE (L25)",
            kovee_event=ctx["event"]["event_id"],
            activation_rows=after,
            required_stages=r["required_stages"],
            replay_byte_identical=True)

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
    second = attention_notice(byom, stream, ctx["event"], f"{tag}-n2")
    effect = second["result"]["eligibility_effect"]
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
    need(True, "")
    prepared_state = kovee.expect_ok(kv(
        "endeavor_promotion_show", None, None,
        {"formation_id": formation}))["result"]
    need(prepared_state["state"] == "prepared",
         f"prepare: {prepared_state}")
    need(prepared_state["slot"]["state"] == "held",
         f"the formation slot is held: {prepared_state}")
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
            "atomic result",
            formation_id=formation, endeavor=endeavor,
            frontier=frontier, assembly=assembly,
            byom_endeavor_rows=1)
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
    byomd's own store beside the daemon."""
    return {
        "act_intent_ref": act["intent_id"],
        "act_intent_digest": json.loads(byom.row(
            "SELECT intent_digest FROM act_intents WHERE intent_id = ?",
            act["intent_id"])),
        "act_revision": revision,
        "subject_digest": act["subject_digest"],
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
    ev.step("kovee: the §16.2 DisclosureManifest is STAGED and COMMITTED "
            "before any authority is asked for — it names the exact "
            "model profile the bytes leave through, the exact items, and "
            "the provider's asserted {region, retention, training_use}",
            disclosure=staged["disclosure_manifest_ref"],
            provider_claims=claims,
            recipient_binding=staged["recipient_binding"],
            model_selector=staged["model_selector"],
            invocation=invocation_id, attempt=attempt, fence_epoch=fence,
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
    effect = driver.ok("effect-show",
                       {"execution_key": act["stable_execution_key"]})
    need(effect["effect"]["state"] == "prepared",
         f"the Effect is committed prepared before any dispatch: {effect}")
    need(effect["attempts"] == [] and effect["consumptions"] == [],
         f"no attempt and no consumption without a permit: {effect}")
    need(byom.count("SELECT COUNT(*) FROM execution_consumption_receipts")
         == 0 and byom.count("SELECT COUNT(*) FROM mandate_uses") == 0,
         "byom minted no receipt and inserted no MandateUse")
    ev.step("kovee broker REFUSES without the permit: byom answers that "
            "the act is only `prepared`, the Effect row is already "
            "COMMITTED prepared (write order = the safety property), "
            "there is NO attempt, NO consumption, NO receipt and NO "
            "MandateUse — and not one byte left the process",
            problem=refused.get("type"), detail=detail,
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

    # A SPENT one-shot permit refuses a second dispatch — kovee's own gate
    # answers before any byte can leave, and byom never sees a second
    # consumption.
    spent = driver.problem("complete", {
        **call_args, **transport["args"],
        "authorization": authorization})
    need("spent" in str(spent.get("detail") or ""),
         f"the one-shot permit must refuse a second dispatch: {spent}")
    need(byom.count("SELECT COUNT(*) FROM mandate_uses") == 1,
         "a refused second dispatch never inserts a second MandateUse")
    after = driver.ok("effect-show",
                      {"execution_key": act["stable_execution_key"]})
    need(len(after["attempts"]) == len(effect["attempts"]),
         f"a refused second dispatch adds no attempt: {after}")

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
            "usage is metered back to byom and BYOM settles it; the exact "
            "retry replays with no second MandateUse and no second "
            "dispatch",
            effect=completion["effect_id"],
            transport_profile=completion["transport_profile"],
            usage=usage, charged=charged,
            byom_receipts=receipts, byom_mandate_uses=uses,
            byom_settlements=1, byom_ledger=ledger,
            kovee_usage_report=report["stable_report_key"],
            settled_by_byom=report["settled_by_byom"],
            byom_measured=measured, byom_charged=charged_q,
            second_dispatch_refused=spent.get("type"),
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


def honesty_labels(ev: Evidence, transport: dict, note: str = ""):
    ev.step("assurance profile, labeled honestly: DEVELOPER — no UID "
            "separation, no attested process identity, no asymmetric "
            "endpoint identity. The gate claims only that the calls it "
            "EXERCISED went through the disclosed, metered broker; "
            "provider-bypass PREVENTION is NOT claimed until K4's secure "
            "profile. Data is synthetic and non-sensitive; no production "
            "effect is performed",
            assurance_profile="developer",
            bypass_prevention_claimed=False,
            confinement_claimed=False,
            data="synthetic, non-sensitive",
            transport_profile=transport["profile"], note=note)


# ------------------------------------------------------------ scripted ----

RECORDING = {
    "model_profile_ref": "mp-anthropic-realm-personal",
    "provider_binding_ref": "mpb-anthropic-realm-personal",
    "profile": "recording-test-double",
    "expect_send_count": 1,
    "args": {"transport": "recording", "reply_body": STUB_REPLY},
}


def mode_scripted() -> int:
    ev = Evidence("i1-flow-scripted")
    print("i1-flow-scripted: the governed loop across both live daemons, "
          "with a STUB provider (no network) so the gate is deterministic")
    ctx = None
    try:
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
        print(f"i1-flow-scripted: PASS ({ev.n} steps; evidence "
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
        for provider, (secret, source) in available:
            ctx = None
            tag = f"i1r{provider['kind'][:3]}"
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
        "When it returns, reply with the word DONE followed by the "
        "identifiers it returned.",
    ])


class HarnessAgent:
    """The agent seat, driven by a REAL harness session per step.

    `one(tool, args)` runs one session and then RECOVERS the reply from
    byomd's own store and event ledger — the same members the scripted MCP
    reply would have carried, read from the daemon that committed them."""

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

    def server(self) -> dict:
        return {"byom": {
            "command": byom_mcp_bin(),
            "args": ["--profile", "participant"],
            "env": {"BYOM_RUNTIME_DIR": str(self.byom.run_dir),
                    "BYOM_PARTICIPANT_TOKEN_FILE":
                        str(self.byom.token_file(
                            f"participant-{AGENT}.token")),
                    "BYOM_SOCIETY": self.society}}}

    def session(self, tool: str, args: dict):
        self.sessions += 1
        n = self.sessions
        prompt = harness_prompt(tool, args)
        allowed = f"mcp__byom__{tool}"
        if self.which == "claude":
            config = self.ev.dir / f"session-{n:02d}-{tool}.mcp.json"
            config.write_text(json.dumps({"mcpServers": self.server()},
                                         indent=1), encoding="utf-8")
            argv = [self.cli, "-p", prompt, "--mcp-config", str(config),
                    "--strict-mcp-config", "--allowedTools", allowed]
        else:
            overrides = []
            for name, spec in self.server().items():
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
            # structurally: with --ignore-user-config the session's ONLY
            # MCP servers are the ones configured here. Both settings are
            # required — an MCP tool call crosses a process boundary that
            # the read-only and workspace-write sandboxes deny.
            argv = [self.cli, "exec", "--skip-git-repo-check",
                    "--ignore-user-config", "-s", "danger-full-access",
                    "-c", 'approval_policy="never"', *overrides, prompt]
        started = time.time()
        session = subprocess.run(argv, capture_output=True, text=True,
                                 stdin=subprocess.DEVNULL,
                                 cwd=str(self.workdir), timeout=900)
        self.ev.blob(f"session-{n:02d}-{tool}.txt",
                     f"$ {' '.join(argv[:2])} ...\n--- exit "
                     f"{session.returncode} after "
                     f"{time.time() - started:.1f}s\n--- allowed tools\n"
                     f"{allowed}\n--- stdout\n{session.stdout}\n"
                     f"--- stderr\n{session.stderr}")
        need(session.returncode == 0,
             f"{self.which} session {n:02d} ({tool}) failed "
             f"({session.returncode}): {session.stderr[-600:]}")

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
        self.session(tool, args)
        return self.recover(tool, args)

    def open(self) -> "HarnessAgent":
        return self

    def call_ok(self, tool: str, args: dict) -> dict:
        return self.one(tool, args)

    def close(self, frames: str | None = None):
        return None

    # -- recovery: byomd's own records, never the session's words --------

    def last(self, kind: str) -> dict:
        rows = [e for e in timeline(self.byom, self.genesis)
                if e["kind"] == kind]
        need(rows, f"byomd's ledger holds no {kind} event")
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
            opened = self.last("activity.opened")
            return {"result": {
                "activity_stream_id": opened["object_ref"],
                "state": "ready"}}
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
            return {"result": {"state": "recorded"}}
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
    "kovee/model_effect@after_dispatch_record",
    "byom/usage_report@before_witness",
    "byom/usage_report@after_finalize",
    "kovee/endeavor_promotion_start@after_commit",
]


def crash_cell_setup(cell: str, ev: Evidence, tag: str) -> dict:
    ctx = governed_setup(ev, tag, {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
    ev.step(f"{cell}: the governed loop is set up to the committed Pledge "
            "on both live daemons",
            society=ctx["society"], episode=ctx["episode"])
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
        refused = driver.problem("complete", {
            **call_args, **RECORDING["args"],
            "authorization": authorization})
        byom.wait_exit()
        byom.start()
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
        ev.step(f"{cell}: killed byomd inside the permit consumption, "
                "restarted; the retry consumed the SAME one-shot permit "
                "and dispatched exactly once — one receipt, one "
                "MandateUse, one attempt, conservation intact",
                refused=refused.get("type"),
                receipts_after_crash=receipts, uses_after_crash=uses,
                receipts_after_retry=1, mandate_uses_after_retry=1,
                attempts=1)
    finally:
        cleanup_live()


def kovee_effect_cell(cell: str, fault: str, ev: Evidence) -> None:
    """The broker's own write order, proven by a real process abort:
    `after_prepare` (the Effect is on disk, no permit consumed, nothing
    sent) and `after_dispatch_record` (the attempt is committed
    `dispatching`, so the outcome is genuinely unknown and must resolve
    AMBIGUOUS with retry frozen)."""
    tag = f"i1c{len(fault) % 7}e"
    ctx = crash_cell_setup(cell, ev, tag)
    try:
        byom, kovee, driver = ctx["byom"], ctx["kovee"], ctx["driver"]
        call_args, act, authorization = armed_broker_state(ctx, tag, "c")
        key = act["stable_execution_key"]
        _, code = driver.run("complete", {
            **call_args, **RECORDING["args"], "fault": fault,
            "authorization": authorization}, expect_ok=False)
        need(code != 0,
             f"{cell}: the armed driver must die, exit was {code}")
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
        # after_dispatch_record: the request MAY have been transmitted.
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
        ev.step(f"{cell}: aborted the broker right after the attempt was "
                "committed `dispatching` — the request MAY have been "
                "transmitted, so koveed's startup sweep resolves it "
                "AMBIGUOUS with retry frozen; the spent one-shot permit "
                "then refuses a second dispatch and byom never sees a "
                "second MandateUse",
                state_after_crash="dispatching",
                state_after_sweep="ambiguous", retry_frozen=True,
                byom_receipts=1, byom_mandate_uses=1, attempts=1)
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
            driver.problem("complete", {
                **call_args, **RECORDING["args"],
                "authorization": authorization})
            byom.wait_exit()
            byom.start()
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
                    "report — the dispatch stands, NOTHING settled, the "
                    "ledger moved no unit beyond the act's own "
                    "reservation, and kovee claims no metering it could "
                    "not report",
                    byom_settlements=0, byom_usage_reports=0,
                    kovee_usage_reports=0, mandate_uses=1, ledger=led)
            return
        # after_finalize, on the Episode's own metered settlement.
        charge = 12
        byom.restart({"BYOMD_ABORT": f"{phase}:usage_report"})
        driver.problem("episode-settle", {
            "stable_binding_key": ctx["bound"]["stable_binding_key"],
            "charge": charge})
        byom.wait_exit()
        byom.start()
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
                "before the reply — the retry REPLAYS the stored "
                "settlement (SettleOnce), replays are byte-identical, and "
                f"exactly {charge} units are committed once",
                byom_settlements=1, charged=charge, ledger=led,
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
    try:
        ctx = governed_setup(ev, tag,
                             {"ANTHROPIC_API_KEY": PLACEHOLDER_KEY},
                             stop_after="activation")
        kovee, byom = ctx["kovee"], ctx["byom"]
        _, _, prepared = formation_prepare(kovee, byom, ctx, tag)
        kovee.restart({"KOVEED_ABORT": f"{phase}:{point}",
                       "ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
        start = kv("endeavor_promotion_start", None, "idem-i1c-start",
                   {"formation_id": prepared,
                    "authentication_observation_ref": "authobs-crash-1"})
        first = kovee.call_raw(start)
        if first is None:
            kovee.wait_exit()
            kovee.start({"ANTHROPIC_API_KEY": PLACEHOLDER_KEY})
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
                "commit point; at most one Endeavor ever exists on byom's "
                "side, the slot never releases with nothing formed, and "
                "the retry reaches `linked` with byte-identical replays",
                formed_after_crash=formed, formed_at_end=1,
                state=view["state"], slot=view["slot"]["state"])
    finally:
        cleanup_live()


def mode_crash_matrix() -> int:
    ev = Evidence("i1-crash")
    print("i1-crash: kill both daemons and the broker chain at the NEW I1 "
          "commit points (BYOMD_ABORT / KOVEED_ABORT / the broker's own "
          "Fault hooks)")
    try:
        byom_permit_cell(CRASH_CELLS[0], "before_witness", ev)
        byom_permit_cell(CRASH_CELLS[1], "after_finalize", ev)
        kovee_effect_cell(CRASH_CELLS[2], "after_prepare", ev)
        kovee_effect_cell(CRASH_CELLS[3], "after_dispatch_record", ev)
        byom_usage_cell(CRASH_CELLS[4], "before_witness", ev)
        byom_usage_cell(CRASH_CELLS[5], "after_finalize", ev)
        kovee_formation_cell(CRASH_CELLS[6], "after_commit", ev)
        print(f"i1-crash: PASS ({len(CRASH_CELLS)} cells; evidence "
              f"{ev.dir.relative_to(REPO)})")
        return 0
    finally:
        ev.close()
        cleanup_live()


# ---------------------------------------------------------------- main ----

def main(argv: list) -> int:
    args = argv[1:]
    try:
        if args == ["--scripted"]:
            return mode_scripted()
        if args == ["--crash-matrix"]:
            return mode_crash_matrix()
        if args == ["--real-model"]:
            return mode_real_model()
        if len(args) == 2 and args[0] == "--harness" \
                and args[1] in ("claude", "codex"):
            return mode_harness(args[1])
        if args == ["--all-checks"]:
            for mode in (mode_scripted, mode_crash_matrix):
                code = mode()
                if code != 0:
                    return code
            print("i1: all checks PASS (scripted + crash-matrix). "
                  "--real-model and --harness are env-gated and run "
                  "separately.")
            return 0
        if len(args) == 3 and args[0] == "--_agent-call":
            return mode_agent_call(args[1], args[2])
    except Fail as e:
        print(f"FAIL  {e}")
        return 1
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
