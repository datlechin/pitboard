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

#[test]
fn an_oversize_credential_is_refused_before_anything_is_written() {
    let svc = format!("{}-oversize", service());
    remove(&svc);
    seed(&svc, "original");

    let err = pitboard_core::testing::vault_write(&common::ctx(), &svc, &"x".repeat(2100))
        .expect_err("a credential past the command limit must be refused");

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
