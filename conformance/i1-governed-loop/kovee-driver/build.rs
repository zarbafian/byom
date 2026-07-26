//! The pinned-revision assertion, made at BUILD time (R3-I02).
//!
//! The driver links kovee's crates through fixed path dependencies, so the
//! kovee tree it is compiled against is decided here and nowhere else. This
//! script:
//!
//!   1. resolves that tree from THIS manifest (never from a name the harness
//!      supplies) and embeds its absolute path;
//!   2. reads its `HEAD` commit and worktree cleanliness and embeds them;
//!   3. REFUSES TO COMPILE when the harness names a different commit in
//!      `$I1_KOVEE_COMMIT` — a driver built against another revision than
//!      the one the gate claims to be gating is exactly the mixed-revision
//!      hazard R3-I02 named, and it is now a build error rather than a
//!      silent pass.
//!
//! The `rerun-if` lines make the embedded commit follow the tree: a new
//! kovee commit moves `refs/heads/*`/`.git/index`, and the harness passes the
//! current commit in the environment, so either change rebuilds this binary.

use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} in {}: {e}", dir.display()));
    if !out.status.success() {
        panic!(
            "git {args:?} in {} failed: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn main() {
    let manifest =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    // The path dependency's own root — the tree whose code this binary will
    // contain. `Cargo.toml`'s `koveed = { path = "../../../../kovee/crates/koveed" }`
    // is the same tree, one level down.
    let kovee = manifest
        .join("../../../../kovee")
        .canonicalize()
        .unwrap_or_else(|e| {
            panic!(
                "the kovee path dependency does not resolve from {}: {e}",
                manifest.display()
            )
        });
    let head = git(&kovee, &["rev-parse", "HEAD"]);
    let dirty = !git(&kovee, &["status", "--porcelain"]).is_empty();
    if let Ok(expected) = std::env::var("I1_KOVEE_COMMIT") {
        let expected = expected.trim().to_owned();
        if !expected.is_empty() && expected != head {
            panic!(
                "the I1 gate names kovee {expected} but {} is at {head}: a driver compiled \
                 against another revision cannot gate the pinned one (R3-I02)",
                kovee.display()
            );
        }
    }
    println!("cargo:rustc-env=I1_KOVEE_COMMIT={head}");
    println!("cargo:rustc-env=I1_KOVEE_PATH={}", kovee.display());
    println!("cargo:rustc-env=I1_KOVEE_DIRTY={dirty}");
    println!("cargo:rerun-if-env-changed=I1_KOVEE_COMMIT");
    let git_dir = kovee.join(".git");
    for path in ["HEAD", "index", "packed-refs"] {
        println!("cargo:rerun-if-changed={}", git_dir.join(path).display());
    }
    let heads = git_dir.join("refs/heads");
    if let Ok(entries) = std::fs::read_dir(&heads) {
        for entry in entries.flatten() {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }
}
