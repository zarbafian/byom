//! `byom-mcp` — an MCP stdio server exposing ONE C3a profile of a local
//! `byomd` to an agent harness (Claude Code, Codex, …): either the
//! 3-tool **candidate** binding (sender-constrained to exactly one
//! MembershipOffer) or the 34-tool **participant** binding, per
//! `mcp/byom-mcp.tools.json` v0.1.1 — never both (amendment A4: the
//! candidate channel closes at admission and never converts in place).
//!
//! The embedded tools document is the contract. Tool names, input
//! schemas, descriptions, and gating flags are parsed from it once at
//! startup and cross-checked against bpp-core's frozen
//! (operation,surface) registry — nothing is hand-copied; a tool absent
//! from the served profile does not exist (deny-by-absence); and every
//! input is validated against its closed document schema before
//! dispatch, so envelope- and channel-derived fields (`version`, `op`,
//! `meta`, `actor_ref`, …) can never ride in on tool input. A document
//! the interpreter cannot fully enforce makes the server refuse to
//! start.
//!
//! The BPP envelope is assembled server-side per the C3a binding
//! envelope (see `bridge`): mutations carry `meta` with a **fresh
//! idempotency key per call**, updates a derived `expected_revision`
//! (RT-01), and the credential travels only as the socket's transport
//! preamble — never as tool input.
//!
//! What you write (a running `byomd` required), e.g. Claude Code:
//! ```text
//! claude mcp add byom -- byom-mcp --profile participant
//! ```
//! Candidate mode additionally needs the offer token:
//! `BYOM_CANDIDATE_TOKEN` (or `BYOM_CANDIDATE_TOKEN_FILE`). Read-only
//! tools say "safe to allow" in their descriptions; keep the ones
//! marked "gated" behind the harness prompt — each is a deliberate yes.
//!
//! Transport: newline-delimited JSON-RPC 2.0 over stdin/stdout (MCP
//! stdio). Stdout carries only protocol messages; logs go to stderr.
//! Tool results are the daemon's success envelope minus `outcome`
//! (`result` plus `revision`/`source_cursor` when minted — the cursor
//! is how an agent enters `byom_events_read`); problems become MCP tool
//! errors carrying the problem type and kind verbatim.

mod bridge;
mod document;
mod validate;

use std::io::{BufReader, Read, Write};

use bridge::BridgeError;
use document::{Document, Profile};
use serde_json::{json, Value};

/// The MCP protocol versions this server implements. `initialize`
/// echoes the client's version only when it is one of these; otherwise
/// it offers the latest.
const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const PROTOCOL_VERSION: &str = SUPPORTED_VERSIONS[0];

/// The largest single JSON-RPC message the server will buffer — bounded
/// so a client cannot grow memory without limit on a newline-free
/// stream.
const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

fn main() {
    let profile = match select_profile() {
        Ok(profile) => profile,
        Err(e) => {
            eprintln!("byom-mcp: refusing to serve: {e}");
            std::process::exit(1);
        }
    };
    let doc = match document::load(profile) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("byom-mcp: refusing to serve: {e}");
            std::process::exit(1);
        }
    };
    let bridge = match bridge::Bridge::from_env(profile) {
        Ok(bridge) => bridge,
        Err(e) => {
            eprintln!("byom-mcp: refusing to serve: {e}");
            std::process::exit(1);
        }
    };
    serve(&doc, &bridge);
}

/// `--profile candidate|participant` (also `--profile=…`), else
/// `$BYOM_MCP_PROFILE`. No default: the two bindings are different
/// credentials, so the operator names one.
fn select_profile() -> Result<Profile, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut named: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--profile" {
            named = args.get(index + 1).cloned();
            index += 2;
        } else if let Some(rest) = arg.strip_prefix("--profile=") {
            named = Some(rest.to_owned());
            index += 1;
        } else {
            return Err(format!(
                "unknown argument {arg:?} (usage: byom-mcp --profile candidate|participant)"
            ));
        }
    }
    let named = match named {
        Some(name) => name,
        None => std::env::var("BYOM_MCP_PROFILE").map_err(|_| {
            "no profile selected: pass --profile candidate|participant \
             or set BYOM_MCP_PROFILE"
                .to_owned()
        })?,
    };
    Profile::parse(&named)
        .ok_or_else(|| format!("unknown profile {named:?} (candidate|participant)"))
}

fn serve(doc: &Document, bridge: &bridge::Bridge) {
    let mut reader = BufReader::new(std::io::stdin());
    let mut out = std::io::stdout();
    loop {
        let line = match read_capped_line(&mut reader, MAX_MESSAGE_BYTES) {
            LineRead::Line(bytes) => bytes,
            LineRead::Eof => break,
            LineRead::TooLarge => {
                // Cannot know the id; answer a parse error (id null).
                let _ = writeln!(out, "{}", error(None, -32700, "message too large"));
                let _ = out.flush();
                break;
            }
            LineRead::Err => break,
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let msg: Value = match serde_json::from_slice(&line) {
            Ok(v) => v,
            Err(_) => {
                // Malformed frame → parse error with a null id (the id
                // is unknowable), so a strict client is not left waiting.
                let _ = writeln!(out, "{}", error(None, -32700, "parse error"));
                let _ = out.flush();
                continue;
            }
        };
        // A message with no `id` is a notification — never reply.
        let id = msg.get("id").cloned();
        let is_notification = id.is_none();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        let response = match method {
            _ if is_notification => None,
            "initialize" => Some(ok(id, initialize_result(&params))),
            "tools/list" => Some(ok(id, json!({ "tools": tool_specs(doc) }))),
            "tools/call" => Some(ok(id, call_tool(doc, bridge, &params))),
            "ping" => Some(ok(id, json!({}))),
            _ => Some(error(id, -32601, "method not found")),
        };
        if let Some(response) = response {
            let _ = writeln!(out, "{response}");
            let _ = out.flush();
        }
    }
}

/// The outcome of reading one newline-terminated message, size-capped.
enum LineRead {
    Line(Vec<u8>),
    Eof,
    TooLarge,
    Err,
}

/// Reads bytes up to the next `\n`, at most `cap` bytes, never growing
/// past it.
fn read_capped_line(reader: &mut impl Read, cap: usize) -> LineRead {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => {
                return if buf.is_empty() {
                    LineRead::Eof
                } else {
                    LineRead::Line(buf)
                }
            }
            Ok(_) => {
                if byte[0] == b'\n' {
                    return LineRead::Line(buf);
                }
                if buf.len() >= cap {
                    return LineRead::TooLarge;
                }
                buf.push(byte[0]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return LineRead::Err,
        }
    }
}

fn ok(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize_result(params: &Value) -> Value {
    // Echo the client's protocol version only when we actually implement
    // it; otherwise offer our latest, so the client never believes we
    // agreed to a version we do not support.
    let version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .filter(|v| SUPPORTED_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "byom-mcp", "version": env!("CARGO_PKG_VERSION") },
    })
}

/// The tool catalogue, straight from the document: name, description
/// (which carries the read-only vs gated marking), and the closed input
/// schema, all verbatim; `readOnlyHint` derives from the document's
/// access flag (`safe_to_allow` ⇒ read-only).
fn tool_specs(doc: &Document) -> Value {
    Value::Array(
        doc.tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                    "annotations": { "readOnlyHint": !tool.gated },
                })
            })
            .collect(),
    )
}

/// Runs a `tools/call`, returning an MCP tool result
/// (`content` + `isError`).
fn call_tool(doc: &Document, bridge: &bridge::Bridge, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    // Deny-by-absence: the served profile's tools are the whole surface.
    let Some(tool) = doc.tool(name) else {
        return tool_text(
            &format!(
                "unknown tool {name:?}: not one of the {} {}-profile \
                 byom-mcp.tools.json tools (deny-by-absence)",
                doc.tools.len(),
                doc.profile.as_str()
            ),
            true,
        );
    };
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    // Validate against the embedded closed schema BEFORE dispatch: an
    // envelope/channel-derived member in the input is refused here.
    if let Err(detail) = validate::validate(&tool.input_schema, &args) {
        return tool_text(&format!("invalid input for {name}: {detail}"), true);
    }
    let Some(args) = args.as_object() else {
        return tool_text(&format!("invalid input for {name}: not an object"), true);
    };
    match bridge.call(&doc.protocol_version, tool, args) {
        Ok(result) => tool_text(&pretty(&result), false),
        Err(BridgeError::Problem(problem)) => tool_text(&problem_text(&problem), true),
        Err(BridgeError::Io(detail)) => tool_text(&detail, true),
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// A daemon problem as tool-error text, the closed problem type
/// (`https://byom.dev/problems/<kind>`) and kind carried verbatim.
fn problem_text(problem: &Value) -> String {
    let kind = problem
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("https://byom.dev/problems/internal");
    let title = problem
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("problem");
    let status = problem.get("status").and_then(Value::as_u64).unwrap_or(0);
    match problem.get("detail").and_then(Value::as_str) {
        Some(detail) => format!("problem {kind} (status {status}): {title} — {detail}"),
        None => format!("problem {kind} (status {status}): {title}"),
    }
}

fn tool_text(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}
