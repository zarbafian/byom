//! BY-J1 (P0): the witness CAS is inter-process atomic and one data
//! directory has exactly one owner.
//!
//! The defect: the "CAS" read the JSONL head and then performed an
//! UNLOCKED append, serialized only by a process-local mutex, with no
//! exclusive data-directory writer lock. Two daemons sharing one data
//! directory could both observe generation N, both append a distinct
//! generation N+1 entry, both receive `Witnessed`, and both finalize.
//! The old "competing CAS" test only checked sequential output from one
//! process, so it could never have seen this.
//!
//! These tests are REAL two-process tests. A `fork` would not do: flock
//! lives on the open file description, which a forked child SHARES, so
//! only genuinely separate processes prove the serialization.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use byom_store::witness::{CasOutcome, Witness, WitnessFault};
use common::TestDaemon;

/// The child half of the competing-CAS test: a SEPARATE process that
/// CASes the same journal at the same prior generation. It is a no-op
/// unless the parent asks for it.
#[test]
fn witness_race_child() {
    let Ok(dir) = std::env::var("BYOM_WITNESS_RACE_DIR") else {
        return;
    };
    let txn = std::env::var("BYOM_WITNESS_RACE_TXN").expect("child transaction id");
    let dir = PathBuf::from(dir);
    // Both children start together.
    let start = dir.join("start");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !start.exists() {
        assert!(Instant::now() < deadline, "the parent never released us");
        std::thread::sleep(Duration::from_millis(5));
    }
    let witness = Witness::open(&dir.join("authority-witness.jsonl")).expect("open witness");
    let outcome = witness
        .cas(
            "inc-shared",
            0,
            "",
            &txn,
            &"a".repeat(64),
            &"b".repeat(64),
            WitnessFault::None,
        )
        .expect("cas");
    match outcome {
        CasOutcome::Witnessed(entry) => {
            println!("RESULT WITNESSED {} {}", entry.generation, txn);
        }
        CasOutcome::HeadConflict { head } => {
            // The loser must not be able to finalize: its transaction is
            // provably absent from the journal.
            assert!(
                witness.query(&txn).expect("query").is_none(),
                "a losing CAS must leave NO journal entry to finalize against"
            );
            println!("RESULT CONFLICT {head} {txn}");
        }
        CasOutcome::Unknown => println!("RESULT UNKNOWN {txn}"),
    }
}

#[test]
fn two_processes_racing_one_journal_produce_exactly_one_winner() {
    let dir = std::env::temp_dir().join(format!(
        "byom-witness-race-{}-{}",
        std::process::id(),
        bpp_core::time::unix_now()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Witness::open(&dir.join("authority-witness.jsonl")).unwrap();

    // A deterministic contention window INSIDE the critical section: the
    // winner holds the journal lock across the whole read/append/fsync,
    // so the loser can only observe the advanced head. Without the
    // inter-process lock both children read head 0 and both append.
    let exe = std::env::current_exe().unwrap();
    let mut children = Vec::new();
    for txn in ["txn-a", "txn-b"] {
        children.push(
            Command::new(&exe)
                .args(["--exact", "witness_race_child", "--nocapture"])
                .env("BYOM_WITNESS_RACE_DIR", &dir)
                .env("BYOM_WITNESS_RACE_TXN", txn)
                .env("BYOM_WITNESS_CAS_DELAY_MS", "400")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("spawn competing witness process"),
        );
    }
    // Release both at once.
    std::thread::sleep(Duration::from_millis(300));
    std::fs::write(dir.join("start"), b"go").unwrap();

    let mut witnessed = Vec::new();
    let mut conflicts = Vec::new();
    for child in children {
        let out = child.wait_with_output().expect("child exit");
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(out.status.success(), "child failed:\n{text}");
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("RESULT WITNESSED ") {
                witnessed.push(rest.to_owned());
            } else if let Some(rest) = line.strip_prefix("RESULT CONFLICT ") {
                conflicts.push(rest.to_owned());
            } else if line.starts_with("RESULT ") {
                panic!("unexpected child outcome: {line}");
            }
        }
    }
    assert_eq!(
        witnessed.len(),
        1,
        "exactly one process may witness generation 1 (witnessed: {witnessed:?}, \
         conflicts: {conflicts:?})"
    );
    assert_eq!(conflicts.len(), 1, "the other must lose the CAS");
    assert!(
        witnessed[0].starts_with("1 "),
        "the winner takes generation 1: {witnessed:?}"
    );

    // The journal itself is the proof: one entry, chain intact.
    let witness = Witness::open(&dir.join("authority-witness.jsonl")).unwrap();
    let entries = witness.entries().expect("the chain verifies");
    assert_eq!(
        entries.len(),
        1,
        "two competing appends would leave two generation-1 entries"
    );
    let loser = if witnessed[0].contains("txn-a") {
        "txn-b"
    } else {
        "txn-a"
    };
    assert!(
        witness.query(loser).unwrap().is_none(),
        "the loser has no receipt and therefore cannot finalize"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_second_daemon_cannot_own_the_same_data_directory() {
    // The first daemon owns the directory for its whole life.
    let daemon = TestDaemon::start("r1-owner");
    let second_run = std::env::temp_dir().join(format!(
        "byomd-r1-owner-run2-{}-{}",
        std::process::id(),
        bpp_core::time::unix_now()
    ));
    std::fs::create_dir_all(&second_run).unwrap();
    let second = Command::new(env!("CARGO_BIN_EXE_byomd"))
        .env("BYOM_DATA_DIR", &daemon.data_dir)
        .env("BYOM_RUNTIME_DIR", &second_run)
        .env_remove("BYOMD_ABORT")
        .output()
        .expect("spawn the second daemon");
    assert!(
        !second.status.success(),
        "a second daemon on one data directory must refuse to start"
    );
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.contains("already owned by another byom endpoint"),
        "the refusal must name exclusive ownership: {stderr}"
    );
    // And the store-level API refuses in-process too.
    let opened = byom_store::Store::open(Path::new(&daemon.data_dir));
    assert!(
        matches!(opened, Err(byom_store::StoreError::DataDirLocked(_))),
        "a second Store must not share the data directory"
    );
    let _ = std::fs::remove_dir_all(&second_run);
}
