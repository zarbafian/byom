//! `byom` — thin operator verbs over the per-surface byomd sockets.
//!
//! ```text
//! byom hello [--surface governance|candidate|participant|projection]
//! byom society bootstrap [--home-authority <ref>] [--charter-ref <ref>]
//! byom society show --society <id>
//! byom membership offer --participant <ref> --society <id> [--standing <ref>]
//!     [--expires-at <rfc3339>] [--decision <ref>]
//! byom participant admit --offer <id> --acceptance <id>
//!     --expected-revision <n> [--decision <ref>]
//! byom events --cursor <continuation> [--page-size <n>]
//! ```
//!
//! Each verb writes one JSON line to the right surface socket and prints
//! the reply. Mutation meta (request id, idempotency key, incarnation)
//! is minted here via `hello`; digests are minted as developer-profile
//! `local_erasure_safe` commitments.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::os::unix::net::UnixStream;

use bpp_core::canonical::{hex, hmac_sha256, tagged_canonical};
use serde_json::{json, Value};

fn socket_path(surface: &str) -> std::path::PathBuf {
    let dir = match std::env::var_os("BYOM_RUNTIME_DIR") {
        Some(dir) if !dir.is_empty() => std::path::PathBuf::from(dir),
        _ => match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(rt) if !rt.is_empty() => std::path::PathBuf::from(rt).join("byom"),
            _ => std::env::temp_dir().join(format!("byom-{}", own_uid())),
        },
    };
    dir.join(format!("{surface}.sock"))
}

fn own_uid() -> u32 {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or(0)
}

fn call(surface: &str, request: &Value, token: Option<&str>) -> Result<Value, String> {
    let path = socket_path(surface);
    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("connect {}: {e} (is byomd running?)", path.display()))?;
    if let Some(token) = token {
        stream
            .write_all(format!("{token}\n").as_bytes())
            .map_err(|e| e.to_string())?;
    }
    stream
        .write_all(format!("{request}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    serde_json::from_str(line.trim_end()).map_err(|e| format!("reply parse: {e}"))
}

fn rand_id(prefix: &str) -> String {
    let mut bytes = [0u8; 8];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    format!("{prefix}-{}", hex(&bytes))
}

/// The daemon's endpoint incarnation, learned via hello (pre-auth).
fn incarnation(surface: &str) -> Result<String, String> {
    let reply = call(surface, &json!({"version": "0.2", "op": "hello"}), None)?;
    reply["result"]["endpoint_incarnation"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("hello failed: {reply}"))
}

fn meta(surface: &str, expected_revision: Option<u64>) -> Result<Value, String> {
    let mut m = json!({
        "request_id": rand_id("req"),
        "idempotency_key": rand_id("idem"),
        "expected_endpoint_incarnation": incarnation(surface)?,
        "expected_recovery_epoch": 0,
    });
    if let Some(rev) = expected_revision {
        m["expected_revision"] = json!(rev);
    }
    Ok(m)
}

/// A developer-profile `local_erasure_safe` commitment over an object
/// under a fresh throwaway secret (the CLI is the committing client).
fn mint_digest(object: &Value, tag: &str) -> Value {
    let mut secret = [0u8; 32];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut secret));
    let preimage = tagged_canonical(tag, object).unwrap_or_default();
    json!({
        "class": "local_erasure_safe",
        "algorithm": "hmac-sha-256",
        "key_ref": rand_id("cli-key"),
        "value_hex": hex(&hmac_sha256(&secret, &preimage)),
    })
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn print_reply(reply: &Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(reply).map_err(|e| e.to_string())?
    );
    if reply["outcome"] == "ok" {
        Ok(())
    } else {
        Err("problem outcome".to_owned())
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut words = args.iter().take_while(|a| !a.starts_with("--"));
    let verb = (
        words.next().map(String::as_str).unwrap_or(""),
        words.next().map(String::as_str).unwrap_or(""),
    );
    match verb {
        ("hello", _) => {
            let surface = flag(&args, "--surface").unwrap_or_else(|| "governance".to_owned());
            let reply = call(&surface, &json!({"version": "0.2", "op": "hello"}), None)?;
            print_reply(&reply)
        }
        ("society", "bootstrap") => {
            // Two governance ops: prepare, then the atomic genesis.
            let home = flag(&args, "--home-authority").unwrap_or_else(|| rand_id("auth-home"));
            let charter_ref = flag(&args, "--charter-ref").unwrap_or_else(|| rand_id("charter"));
            let class_ref =
                flag(&args, "--classification-ref").unwrap_or_else(|| rand_id("class-bind"));
            let prepare = json!({
                "version": "0.2", "op": "society_prepare",
                "meta": meta("governance", None)?,
                "home_authority_ref": home,
                "proposed_charter_ref": charter_ref,
                "proposed_charter_digest":
                    mint_digest(&json!({"charter_ref": charter_ref}), "bpp-charter-body-v0"),
                "classification_binding_ref": class_ref,
                "classification_binding_digest":
                    mint_digest(&json!({"binding_ref": class_ref}), "bpp-classification-v0"),
            });
            let prepared = call("governance", &prepare, None)?;
            print_reply(&prepared)?;
            let result = &prepared["result"];
            let bootstrap = json!({
                "version": "0.2", "op": "society_bootstrap",
                "meta": meta("governance", result["revision"].as_u64())?,
                "society_id": result["society_id"],
                "preparation_ref": result["preparation_ref"],
                "subject_digest": result["subject_digest"],
            });
            let reply = call("governance", &bootstrap, None)?;
            print_reply(&reply)
        }
        ("society", "show") => {
            let society = flag(&args, "--society").ok_or("--society required")?;
            let reply = call(
                "projection",
                &json!({"version": "0.2", "op": "society_show", "society_id": society}),
                None,
            )?;
            print_reply(&reply)
        }
        ("membership", "offer") => {
            let participant = flag(&args, "--participant").ok_or("--participant required")?;
            let standing = flag(&args, "--standing").unwrap_or_else(|| rand_id("standing-p"));
            // The offer is authorized by the Society's immutable genesis
            // GovernanceDecision (BY-A1): an invented reference fails
            // closed with decision_incomplete, so name it or the Society.
            let decision = match flag(&args, "--decision") {
                Some(decision) => decision,
                None => format!(
                    "dec-society-{}",
                    flag(&args, "--society").ok_or(
                        "--society (or --decision) required: membership_offer resolves the \
                         Society's genesis GovernanceDecision dec-society-<society-id>"
                    )?
                ),
            };
            let expires = flag(&args, "--expires-at").unwrap_or_else(|| {
                bpp_core::time::rfc3339_utc(bpp_core::time::unix_now() + 86_400)
            });
            let subject = json!({
                "participant_ref": participant,
                "proposed_standing_ref": standing,
            });
            let request = json!({
                "version": "0.2", "op": "membership_offer",
                "meta": meta("governance", None)?,
                "participant_ref": participant,
                "proposed_standing_ref": standing,
                "subject_digest": mint_digest(&subject, "bpp-offer-subject-v0"),
                "offered_by_decision_ref": decision,
                "expires_at": expires,
            });
            let reply = call("governance", &request, None)?;
            print_reply(&reply)?;
            if let Some(offer_id) = reply["result"]["offer_id"].as_str() {
                eprintln!(
                    "candidate channel credential file (public binding only, 0600; \
                     the holder claims its peer-bound proof key over the socket): \
                     <data-dir>/channels/candidate-{offer_id}.token"
                );
            }
            Ok(())
        }
        ("participant", "admit") => {
            let offer = flag(&args, "--offer").ok_or("--offer required")?;
            let acceptance = flag(&args, "--acceptance").ok_or("--acceptance required")?;
            let revision: u64 = flag(&args, "--expected-revision")
                .ok_or("--expected-revision required")?
                .parse()
                .map_err(|_| "--expected-revision must be an integer")?;
            // Admission resolves the immutable decision byom formed for
            // THIS offer at membership_offer time (BY-A1).
            let decision =
                flag(&args, "--decision").unwrap_or_else(|| format!("dec-offer-{offer}"));
            let subject_digest = match flag(&args, "--subject-digest") {
                Some(text) => serde_json::from_str(&text)
                    .map_err(|e| format!("--subject-digest parse: {e}"))?,
                None => mint_digest(&json!({"offer_ref": offer}), "bpp-admission-subject-v0"),
            };
            let request = json!({
                "version": "0.2", "op": "participant_admit",
                "meta": meta("governance", Some(revision))?,
                "offer_ref": offer,
                "membership_acceptance_ref": acceptance,
                "admitted_by_decision_ref": decision,
                "admission_subject_digest": subject_digest,
            });
            let reply = call("governance", &request, None)?;
            print_reply(&reply)
        }
        ("events", _) => {
            let cursor = flag(&args, "--cursor").ok_or(
                "--cursor required (society_bootstrap's source_cursor replays from genesis)",
            )?;
            let page: u64 = flag(&args, "--page-size")
                .unwrap_or_else(|| "512".to_owned())
                .parse()
                .map_err(|_| "--page-size must be an integer")?;
            let reply = call(
                "projection",
                &json!({"version": "0.2", "op": "events_read",
                        "continuation": cursor, "page_size": page}),
                None,
            )?;
            print_reply(&reply)
        }
        _ => Err(concat!(
            "usage: byom hello [--surface s] | byom society bootstrap | ",
            "byom society show --society <id> | byom membership offer --participant <ref> | ",
            "byom participant admit --offer <id> --acceptance <id> --expected-revision <n> | ",
            "byom events --cursor <continuation>"
        )
        .to_owned()),
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("byom: {e}");
        std::process::exit(1);
    }
}
