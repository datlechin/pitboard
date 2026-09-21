//! Shared guards for tests that touch the real keychain.
//!
//! Getting this wrong costs the user a login, so the rule is mandatory rather than
//! remembered: every service name a test writes must be derived from that test's own
//! identity, which Rust already forbids two tests in a module from sharing.

/// Refuse a service name that this machine's Claude Code would actually read.
///
/// A slot hashed from a scratch directory is safe by construction and is exactly what the
/// round-trip tests need, so the family as a whole is not off limits — only the two names
/// that resolve to a real login here.
pub fn guard_not_live(service: &str) {
    assert_ne!(
        service,
        pitboard::slot::LIVE_SERVICE,
        "a test must never address the default credential slot"
    );
    assert_ne!(
        service,
        pitboard::claude::live_service(),
        "a test must never address the slot this machine's Claude Code reads"
    );
}

#[test]
fn the_guard_refuses_the_slots_that_hold_a_real_login() {
    guard_not_live("pitboard-citest-1");
    guard_not_live("Claude Code-credentials-deadbeef");

    let caught = std::panic::catch_unwind(|| guard_not_live(pitboard::slot::LIVE_SERVICE));
    assert!(caught.is_err(), "the default slot must be refused");
    let caught = std::panic::catch_unwind(|| guard_not_live(&pitboard::claude::live_service()));
    assert!(caught.is_err(), "this machine's live slot must be refused");
}
