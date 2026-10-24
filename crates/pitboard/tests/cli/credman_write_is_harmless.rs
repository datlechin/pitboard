#![cfg(windows)]

//! Pitboard writes Claude Code's login into Credential Manager as `Bun.secrets` writes it for
//! Claude Code (`credman_attributes`, RF-G3's values): a generic item, enterprise persistence,
//! the item's own name after the service as its user name, no comment and no attributes. A
//! write that changed one would leave an item Claude Code's own write and read do not expect.
//! The counterpart of keychain_write_is_harmless.rs. Runs only against `pitboard-citest-*`
//! items, never anything Claude Code owns.

use crate::common;
use pitboard_core::testing::credman::{self, Item};

const GENERIC: u32 = 1;
const ENTERPRISE: u32 = 3;

fn service() -> String {
    format!("pitboard-citest-{}", std::process::id())
}

fn as_bun_writes(item: &Item, user: &str) {
    assert_eq!(item.kind, GENERIC, "{user}");
    assert_eq!(item.persist, ENTERPRISE, "{user}");
    assert_eq!(item.user, user);
    assert_eq!(item.comment, None, "{user}");
    assert_eq!(item.attributes, 0, "{user}");
}

// Credential Manager can still show an item just after its delete (docs/facts/windows-runners.md).
fn gone(name: &str) -> bool {
    (0..6).any(|again| {
        if again > 0 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        credman::item(name).expect("read").is_none()
    })
}

// Taken away by name however the test ends: the leak check after the run finds none.
struct Discarded(pitboard_core::context::Context, String);

impl Drop for Discarded {
    fn drop(&mut self) {
        if let Err(why) = credman::sweep(&self.0, &self.1) {
            eprintln!("{} was not swept: {why}", self.1);
        }
    }
}

#[test]
fn writing_leaves_each_item_as_bun_writes_one() {
    let svc = service();
    // Homes of its own: the gate refuses the test's environment, which withholds USERPROFILE.
    let env = common::Env::new("credman-write");
    let ctx = env.context();
    let _discarded = Discarded(ctx.clone(), svc.clone());
    let whole = format!("{svc}/claude-code-user");
    credman::write_as_bun(&whole, "claude-code-user", r#"{"seed":true}"#).expect("seeded");
    as_bun_writes(
        &credman::item(&whole).expect("read").expect("seeded"),
        "claude-code-user",
    );

    let small = r#"{"claudeAiOauth":{"accessToken":"a","refreshToken":"b","expiresAt":1}}"#;
    credman::write(&ctx, &svc, small).expect("the production writer should succeed");
    let written = credman::item(&whole).expect("read").expect("written");
    as_bun_writes(&written, "claude-code-user");
    assert_eq!(written.bytes, small.len());
    assert_eq!(
        credman::read(&ctx, &svc).expect("read").as_deref(),
        Some(small)
    );

    // 9000 bytes are 12000 characters of base64: five pieces.
    let large = format!(r#"{{"mcpOAuth":"{}"}}"#, "x".repeat(9000 - 15));
    assert_eq!(large.len(), 9000);
    credman::write(&ctx, &svc, &large).expect("written in pieces");
    for piece in ["#0", "#1", "#2", "#3", "#4", "#m"] {
        let user = format!("claude-code-user{piece}");
        let item = credman::item(&format!("{svc}/{user}"))
            .expect("read")
            .unwrap_or_else(|| panic!("{piece} is there"));
        as_bun_writes(&item, &user);
    }
    for piece in ["", "#5", "#p"] {
        assert!(gone(&format!("{whole}{piece}")), "{piece:?} is not left");
    }
    assert_eq!(credman::read(&ctx, &svc).expect("read"), Some(large));

    credman::discard(&ctx, &svc).expect("deleted");
    for piece in ["", "#0", "#4", "#m", "#p"] {
        assert!(gone(&format!("{whole}{piece}")), "{piece:?} is discarded");
    }
}

// Refused before Credential Manager is asked: every name a real login may be kept under.
#[test]
fn a_test_build_never_addresses_a_real_login() {
    for name in [
        "Claude Code-credentials/claude-code-user",
        "Claude Code-credentials-e80beed8/claude-code-user#m",
        "cli|0123456789abcdef.Codex Auth",
    ] {
        assert!(credman::item(name).is_err(), "{name}");
        assert!(
            credman::write_as_bun(name, "claude-code-user", "{}").is_err(),
            "{name}"
        );
    }
    // Homes of its own, so the gate lets the discard through to the guard.
    let env = common::Env::new("credman-refused");
    let ctx = env.context();
    for refused in [
        credman::read(&ctx, "Claude Code-credentials").map(drop),
        credman::discard(&ctx, "Claude Code-credentials"),
    ] {
        let why = refused.expect_err("refused").to_string();
        assert!(why.contains("a test must never address"), "{why}");
    }
}
