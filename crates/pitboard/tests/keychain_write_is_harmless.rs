#![cfg(target_os = "macos")]

//! Proves the production writer leaves a keychain item exactly as it found it.
//!
//! A foreign in-process write replaces the item's partition list with the caller's code
//! hash, evicting `apple-tool:`. Nothing errors; every later `/usr/bin/security` read of
//! that item just costs 1-3 seconds instead of 0.02. Claude Code re-reads the credential
//! store behind a 30-second cache, so that would be a permanent stall every half minute.
//!
//! Read latency is measured rather than the ACL dumped, because latency is the damage.
//! Runs only against `pitboard-citest-*` items, never anything Claude Code owns.

mod common;

use common::{account, guard_not_live};
use std::process::Command;
use std::time::{Duration, Instant};

const SECURITY: &str = "/usr/bin/security";
/// An unpoisoned read is ~20ms; a poisoned one is measured in seconds.
const POISONED: Duration = Duration::from_millis(200);

fn service() -> String {
    format!("pitboard-citest-{}", std::process::id())
}

fn seed(service: &str, value: &str) {
    guard_not_live(service);
    let ok = Command::new(SECURITY)
        .args([
            "add-generic-password",
            "-U",
            "-a",
            &account(),
            "-s",
            service,
            "-w",
            value,
        ])
        .status()
        .expect("exec security")
        .success();
    assert!(ok, "could not create the test item");
}

fn attributes(service: &str) -> String {
    let out = Command::new(SECURITY)
        .args(["find-generic-password", "-a", &account(), "-s", service])
        .output()
        .expect("exec security");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.contains("\"mdat\""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn timed_read(service: &str) -> (String, Duration) {
    let t = Instant::now();
    let out = Command::new(SECURITY)
        .args([
            "find-generic-password",
            "-a",
            &account(),
            "-s",
            service,
            "-w",
        ])
        .output()
        .expect("exec security");
    (
        String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        t.elapsed(),
    )
}

fn remove(service: &str) {
    let _ = Command::new(SECURITY)
        .args(["delete-generic-password", "-a", &account(), "-s", service])
        .output();
}

#[test]
fn writing_preserves_attributes_and_does_not_slow_later_reads() {
    let svc = service();
    remove(&svc);
    seed(&svc, "{\"seed\":true}");

    let before_attrs = attributes(&svc);
    let (_, baseline) = timed_read(&svc);

    let payload = r#"{"claudeAiOauth":{"accessToken":"a","refreshToken":"b","expiresAt":1}}"#;
    let wrote = pitboard_core::testing::vault_write(&common::ctx(), &svc, payload);

    let (back, after) = timed_read(&svc);
    let after_attrs = attributes(&svc);
    remove(&svc);

    wrote.expect("the production writer should succeed");
    assert_eq!(back, payload, "the value must round-trip byte for byte");
    assert_eq!(
        before_attrs, after_attrs,
        "every attribute except mdat must survive the write"
    );
    assert!(
        after < POISONED,
        "read cost {after:?} after our write (was {baseline:?}); \
         the partition list looks poisoned, which would tax Claude Code on every re-read"
    );
}

/// A login past what `security` reads from stdin has one route left, the argument line,
/// which is what Claude Code uses for the same login. What matters is that taking it costs
/// nothing afterwards: the item must still read as fast as one written any other way, or
/// Claude Code pays for pitboard's write on every re-read.
#[test]
fn a_credential_past_the_stdin_limit_is_written_without_taxing_later_reads() {
    let svc = format!("{}-oversize", service());
    remove(&svc);
    seed(&svc, "original");
    let (_, baseline) = timed_read(&svc);

    let big = "x".repeat(2100);
    pitboard_core::testing::vault_write(&common::ctx(), &svc, &big)
        .expect("the argument line is the only way to write one this size");

    let (back, after) = timed_read(&svc);
    remove(&svc);
    assert_eq!(back, big, "the value must round-trip byte for byte");
    assert!(
        after < POISONED,
        "read cost {after:?} after the write (was {baseline:?}); \
         a write must never make later reads expensive"
    );
}

/// Anyone who would rather refuse than have the login on an argument line can say so, and
/// then nothing is written at all.
#[test]
fn the_argument_line_can_be_refused() {
    let svc = format!("{}-refused", service());
    remove(&svc);
    seed(&svc, "original");

    let refusing = common::ctx().with_argv_fallback(false);
    let err = pitboard_core::testing::vault_write(&refusing, &svc, &"x".repeat(2100))
        .expect_err("refused, because it cannot go through stdin");

    let (still, _) = timed_read(&svc);
    remove(&svc);
    assert!(
        err.to_string().contains("command limit"),
        "unexpected error: {err}"
    );
    assert_eq!(
        still, "original",
        "a refused write must not have changed anything"
    );
}
